//! Sample renderers for the block sounds: low-passed noise plus an
//! optional tone under an exponential envelope, one recipe per material.

use ox_core::blocks::Material;
use ox_core::rng::Lcg;

use super::SoundKind;

struct Recipe {
    seconds: f32,
    noise: f32,
    lowpass: f32,
    tone_hz: f32,
    tone: f32,
    decay: f32,
}

struct Shape {
    gain: f32,
    length: f32,
    pitch: f32,
}

const MASTER: f32 = 0.5;
const FADE_S: f32 = 0.002;

const fn recipe(material: Material) -> Recipe {
    match material {
        Material::Stone | Material::Unbreakable => Recipe {
            seconds: 0.12,
            noise: 1.2,
            lowpass: 0.6,
            tone_hz: 0.0,
            tone: 0.0,
            decay: 18.0,
        },
        Material::Dirt => Recipe {
            seconds: 0.18,
            noise: 1.4,
            lowpass: 0.15,
            tone_hz: 0.0,
            tone: 0.0,
            decay: 12.0,
        },
        Material::Sand => Recipe {
            seconds: 0.16,
            noise: 1.4,
            lowpass: 0.12,
            tone_hz: 0.0,
            tone: 0.0,
            decay: 12.0,
        },
        Material::Wood => Recipe {
            seconds: 0.16,
            noise: 0.8,
            lowpass: 0.3,
            tone_hz: 200.0,
            tone: 0.5,
            decay: 14.0,
        },
        Material::Leaves | Material::Snow => Recipe {
            seconds: 0.22,
            noise: 1.0,
            lowpass: 0.5,
            tone_hz: 0.0,
            tone: 0.0,
            decay: 9.0,
        },
        Material::Glass => Recipe {
            seconds: 0.25,
            noise: 1.0,
            lowpass: 0.85,
            tone_hz: 2400.0,
            tone: 0.35,
            decay: 10.0,
        },
    }
}

const fn shape(sound: SoundKind) -> (Material, Shape, u32) {
    match sound {
        SoundKind::Dig(m) => (
            m,
            Shape {
                gain: 0.45,
                length: 0.6,
                pitch: 1.0,
            },
            0,
        ),
        SoundKind::Break(m) => (
            m,
            Shape {
                gain: 1.0,
                length: 1.4,
                pitch: 0.9,
            },
            1,
        ),
        SoundKind::Place(m) => (
            m,
            Shape {
                gain: 0.7,
                length: 0.8,
                pitch: 0.85,
            },
            2,
        ),
    }
}

/// Mono samples for `sound` at `pitch` (1 = nominal) and `sample_rate`;
/// bit-identical for equal inputs.
pub(crate) fn render(sound: SoundKind, pitch: f32, sample_rate: u32) -> Vec<f32> {
    let (material, shape, index) = shape(sound);
    let recipe = recipe(material);
    let pitch = pitch * shape.pitch;
    let rate = sample_rate as f32;
    let count = (recipe.seconds * shape.length / pitch * rate) as usize;
    let fade = ((FADE_S * rate) as usize).max(1);
    let alpha = (recipe.lowpass * pitch).min(1.0);
    let mut rng = Lcg::new((material as u32) * 16 + index);
    let mut low = 0.0;
    (0..count)
        .map(|i| {
            let t = i as f32 / rate;
            let white = rng.range(-1.0, 1.0);
            low += alpha * (white - low);
            let tone = (std::f32::consts::TAU * recipe.tone_hz * pitch * t).sin()
                * recipe.tone
                * (-3.0 * recipe.decay * t).exp();
            let envelope = (-recipe.decay * t / shape.length).exp();
            let tail = (count - 1 - i).min(fade) as f32 / fade as f32;
            (low * recipe.noise + tone) * envelope * shape.gain * MASTER * tail
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATERIALS: [Material; 8] = [
        Material::Stone,
        Material::Dirt,
        Material::Sand,
        Material::Wood,
        Material::Leaves,
        Material::Glass,
        Material::Snow,
        Material::Unbreakable,
    ];

    #[test]
    fn render_is_finite_and_bounded() {
        for m in MATERIALS {
            for sound in [SoundKind::Dig(m), SoundKind::Break(m), SoundKind::Place(m)] {
                let samples = render(sound, 1.0, 48_000);
                assert!(!samples.is_empty());
                assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                assert!(
                    samples.iter().any(|s| s.abs() > 0.01),
                    "{sound:?} is silent"
                );
            }
        }
    }

    #[test]
    fn break_is_longer_than_dig() {
        let dig = render(SoundKind::Dig(Material::Stone), 1.0, 44_100);
        let brk = render(SoundKind::Break(Material::Stone), 1.0, 44_100);
        assert!(brk.len() > dig.len());
    }

    #[test]
    fn same_inputs_render_identically() {
        let a = render(SoundKind::Place(Material::Wood), 1.05, 44_100);
        let b = render(SoundKind::Place(Material::Wood), 1.05, 44_100);
        let bits = |v: &[f32]| v.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b));
    }

    #[test]
    fn higher_pitch_renders_fewer_samples() {
        let low = render(SoundKind::Dig(Material::Dirt), 0.9, 44_100);
        let high = render(SoundKind::Dig(Material::Dirt), 1.1, 44_100);
        assert!(high.len() < low.len());
    }

    #[test]
    fn samples_end_faded() {
        let samples = render(SoundKind::Break(Material::Glass), 1.0, 44_100);
        assert_eq!(samples.last().map(|s| s.to_bits()), Some(0.0f32.to_bits()));
    }
}
