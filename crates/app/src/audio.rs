//! Procedural sound: one cpal output stream mixing short synthesized
//! voices for dig, break and place, one recipe per material. Nothing is
//! read from disk, and the headless modes never open it.

mod synth;

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ox_core::blocks::Material;
use ox_core::rng::Lcg;

/// One block interaction to voice, by the block's material.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SoundKind {
    /// A dig hit landed on a standing block.
    Dig(Material),
    /// A block broke.
    Break(Material),
    /// A block was placed.
    Place(Material),
}

const MAX_VOICES: usize = 16;

#[derive(Default)]
struct Voice {
    samples: Vec<f32>,
    cursor: usize,
    /// Order the voice was started in; the lowest is evicted first.
    stamp: u64,
}

impl Voice {
    const fn done(&self) -> bool {
        self.cursor >= self.samples.len()
    }
}

/// The voices the output callback owns. Slots are fixed and buffers are
/// swapped in and out, never allocated or freed here: the callback runs
/// under a hard deadline and must not touch the allocator.
struct Mixer {
    voices: [Voice; MAX_VOICES],
    started: u64,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            voices: std::array::from_fn(|_| Voice::default()),
            started: 0,
        }
    }
}

impl Mixer {
    /// Starts `samples` in the first finished slot, or the slot holding the
    /// oldest voice, and hands back the buffer it displaced for the caller
    /// to free off the audio thread.
    fn add(&mut self, samples: Vec<f32>) -> Vec<f32> {
        let slot = self
            .voices
            .iter()
            .position(Voice::done)
            .unwrap_or_else(|| self.oldest());
        self.started += 1;
        let voice = &mut self.voices[slot];
        voice.cursor = 0;
        voice.stamp = self.started;
        std::mem::replace(&mut voice.samples, samples)
    }

    fn oldest(&self) -> usize {
        let mut best = 0;
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.stamp < self.voices[best].stamp {
                best = i;
            }
        }
        best
    }

    /// Sums every voice into `out`, the same signal on all `channels`,
    /// clamped to `[-1, 1]`. Finished voices stop contributing; their slots
    /// are reused by [`Mixer::add`].
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        for frame in out.chunks_mut(channels.max(1)) {
            let mut sum = 0.0;
            for voice in &mut self.voices {
                if let Some(sample) = voice.samples.get(voice.cursor) {
                    sum += sample;
                    voice.cursor += 1;
                }
            }
            frame.fill(sum.clamp(-1.0, 1.0));
        }
    }
}

/// Buffers in flight between the game thread and the output callback: the
/// game thread fills `incoming` and empties `spent`, the callback does the
/// reverse. Both are sized once so neither side allocates while holding the
/// lock.
struct Handoff {
    incoming: Vec<Vec<f32>>,
    spent: Vec<Vec<f32>>,
}

impl Handoff {
    fn new() -> Self {
        Self {
            incoming: Vec::with_capacity(MAX_VOICES),
            spent: Vec::with_capacity(MAX_VOICES),
        }
    }
}

/// An open output stream plus the queue feeding it; dropping it stops
/// playback.
pub(crate) struct Audio {
    _stream: cpal::Stream,
    handoff: Arc<Mutex<Handoff>>,
    /// Swapped with `Handoff::spent` so reclaimed buffers leave the lock
    /// with their capacity intact.
    reclaimed: Vec<Vec<f32>>,
    sample_rate: u32,
    rng: Lcg,
}

impl Audio {
    /// Opens the default output device. `None` when there is no device,
    /// the device refuses its default config or a stream, or the default
    /// sample format is not `f32`; the game then runs silent.
    pub(crate) fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        if supported.sample_format() != cpal::SampleFormat::F32 {
            return None;
        }
        let channels = usize::from(supported.channels());
        let sample_rate = supported.sample_rate();
        let handoff = Arc::new(Mutex::new(Handoff::new()));
        let queue = Arc::clone(&handoff);
        let mut mixer = Mixer::default();
        let stream = device
            .build_output_stream(
                supported.config(),
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if let Ok(mut queue) = queue.try_lock() {
                        let Handoff { incoming, spent } = &mut *queue;
                        for samples in incoming.drain(..) {
                            spent.push(mixer.add(samples));
                        }
                    }
                    mixer.fill(out, channels);
                },
                |_| {},
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Self {
            _stream: stream,
            handoff,
            reclaimed: Vec::with_capacity(MAX_VOICES),
            sample_rate,
            rng: Lcg::new(0x000A_0D10),
        })
    }

    /// Synthesizes `sound` at a slightly random pitch and queues it, then
    /// frees the buffers the callback finished with.
    pub(crate) fn play(&mut self, sound: SoundKind) {
        let pitch = self.rng.range(0.9, 1.1);
        let samples = synth::render(sound, pitch, self.sample_rate);
        if let Ok(mut queue) = self.handoff.lock() {
            if queue.incoming.len() < MAX_VOICES {
                queue.incoming.push(samples);
            }
            std::mem::swap(&mut queue.spent, &mut self.reclaimed);
        }
        self.reclaimed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(mixer: &Mixer) -> usize {
        mixer.voices.iter().filter(|v| !v.done()).count()
    }

    #[test]
    fn mixer_sums_voices_and_stops_finished_ones() {
        let mut mixer = Mixer::default();
        let _ = mixer.add(vec![0.25, 0.25, 0.25]);
        let _ = mixer.add(vec![0.5]);
        let mut out = [9.0f32; 4];
        mixer.fill(&mut out, 2);
        assert_eq!(
            out.map(f32::to_bits),
            [0.75f32, 0.75, 0.25, 0.25].map(f32::to_bits)
        );
        assert_eq!(live(&mixer), 1);
        let mut rest = [9.0f32; 4];
        mixer.fill(&mut rest, 2);
        assert_eq!(
            rest.map(f32::to_bits),
            [0.25f32, 0.25, 0.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(live(&mixer), 0);
    }

    #[test]
    fn mixer_clamps_and_caps_voices() {
        let mut mixer = Mixer::default();
        for _ in 0..(MAX_VOICES + 4) {
            let _ = mixer.add(vec![0.9; 2]);
        }
        assert_eq!(live(&mixer), MAX_VOICES);
        let mut out = [0.0f32; 2];
        mixer.fill(&mut out, 1);
        assert_eq!(out.map(f32::to_bits), [1.0f32, 1.0].map(f32::to_bits));
    }

    #[test]
    fn add_reuses_a_finished_slot_before_evicting_a_live_one() {
        let mut mixer = Mixer::default();
        for i in 0..MAX_VOICES {
            let _ = mixer.add(vec![0.1; if i == 3 { 1 } else { 8 }]);
        }
        let mut out = [0.0f32; 2];
        mixer.fill(&mut out, 1);
        assert_eq!(live(&mixer), MAX_VOICES - 1);
        let _ = mixer.add(vec![0.2; 8]);
        assert_eq!(live(&mixer), MAX_VOICES);
        assert_eq!(mixer.voices[3].samples[0].to_bits(), 0.2f32.to_bits());
        assert_eq!(mixer.voices[3].cursor, 0);
    }

    #[test]
    fn add_hands_back_the_buffer_it_displaced() {
        let mut mixer = Mixer::default();
        assert!(mixer.add(vec![0.5; 4]).is_empty());
        for _ in 0..MAX_VOICES {
            let _ = mixer.add(vec![0.5; 4]);
        }
        assert_eq!(mixer.add(vec![0.5; 4]).len(), 4);
    }

    #[test]
    fn eviction_takes_the_oldest_voice() {
        let mut mixer = Mixer::default();
        for _ in 0..MAX_VOICES {
            let _ = mixer.add(vec![0.5; 8]);
        }
        let oldest = mixer.oldest();
        assert_eq!(oldest, 0);
        let _ = mixer.add(vec![0.25; 8]);
        assert_eq!(mixer.voices[0].samples[0].to_bits(), 0.25f32.to_bits());
        assert_eq!(mixer.oldest(), 1);
    }
}
