//! What the UI pass already uploaded, so a menu nobody touches redraws
//! from the buffers as they stand.

use super::{Overlay, UiBuilder, UiState};

/// Everything [`super::menu::draw`] reads. Two frames sharing one of these
/// draw the same overlay, down to the vertex.
#[derive(Debug, PartialEq)]
pub(super) struct OverlayKey {
    overlay: Overlay,
    cursor: Option<[f32; 2]>,
    scroll: f32,
    version: String,
    width: f32,
    height: f32,
}

impl OverlayKey {
    /// The key of an overlay frame, or `None` for a HUD frame: the HUD
    /// carries the frame counter and the player position, so it is rebuilt
    /// every frame and never keyed.
    pub(super) fn of(state: &UiState<'_>, width: f32, height: f32) -> Option<Self> {
        Some(Self {
            overlay: state.overlay?,
            cursor: state.cursor,
            scroll: state.scroll,
            version: state.version.to_owned(),
            width,
            height,
        })
    }
}

/// Geometry from the last recorded frame, still loaded in the buffers.
pub(super) struct Recorded {
    /// Inputs that produced it, or `None` for the HUD, which never matches.
    pub(super) key: Option<OverlayKey>,
    pub(super) builder: UiBuilder,
}

impl Recorded {
    /// Whether the buffers still hold the geometry `key` calls for. A HUD
    /// frame is unkeyed on both sides: it never reuses what is loaded, and
    /// it overwrites the buffers, so the next overlay frame rebuilds.
    pub(super) fn holds(loaded: Option<&Self>, key: Option<&OverlayKey>) -> bool {
        key.is_some() && loaded.and_then(|r| r.key.as_ref()) == key
    }
}

#[cfg(test)]
mod tests {
    use super::{OverlayKey, Recorded, UiBuilder};
    use crate::pass::ui::{Overlay, UiState};

    const SIZE: (f32, f32) = (1280.0, 720.0);

    fn state(overlay: Option<Overlay>) -> UiState<'static> {
        UiState {
            overlay,
            selected: 0,
            selected_name: "GRASS",
            item_name_alpha: 0.0,
            fps: 60,
            pos: [0.0, 70.0, 0.0],
            mode: "ground",
            cursor: None,
            scroll: 0.0,
            version: "oxcraft test-a",
        }
    }

    fn key(overlay: Option<Overlay>) -> Option<OverlayKey> {
        OverlayKey::of(&state(overlay), SIZE.0, SIZE.1)
    }

    /// A recorded frame carrying `key` and no geometry.
    fn recorded(key: Option<OverlayKey>) -> Recorded {
        Recorded {
            key,
            builder: UiBuilder::default(),
        }
    }

    #[test]
    fn the_hud_is_never_keyed_so_it_rebuilds_every_frame() {
        assert!(key(None).is_none());
    }

    #[test]
    fn an_untouched_overlay_keeps_its_key() {
        assert_eq!(key(Some(Overlay::Title)), key(Some(Overlay::Title)));
        assert!(key(Some(Overlay::Title)).is_some());
    }

    #[test]
    fn every_input_the_menu_reads_changes_the_key() {
        let base = state(Some(Overlay::Title));
        let mut moved = state(Some(Overlay::Title));
        moved.cursor = Some([100.0, 200.0]);
        let mut renamed = state(Some(Overlay::Title));
        renamed.version = "oxcraft test-b";
        let mut scrolled = state(Some(Overlay::Controls));
        scrolled.scroll = 40.0;
        let of = |s: &UiState<'_>| OverlayKey::of(s, SIZE.0, SIZE.1);
        for other in [state(Some(Overlay::Pause)), moved, renamed] {
            assert_ne!(of(&base), of(&other));
        }
        assert_ne!(of(&scrolled), key(Some(Overlay::Controls)));
        assert_ne!(of(&base), OverlayKey::of(&base, 640.0, SIZE.1));
        assert_ne!(of(&base), OverlayKey::of(&base, SIZE.0, 360.0));
    }

    #[test]
    fn an_untouched_overlay_redraws_from_the_loaded_buffers() {
        let title = recorded(key(Some(Overlay::Title)));
        assert!(Recorded::holds(Some(&title), title.key.as_ref()));
    }

    #[test]
    fn a_hud_frame_between_two_overlay_frames_forces_a_rebuild() {
        let title = key(Some(Overlay::Title));
        let hud = recorded(None);
        assert!(
            !Recorded::holds(Some(&hud), title.as_ref()),
            "the HUD overwrote them"
        );
        assert!(
            !Recorded::holds(Some(&recorded(key(Some(Overlay::Title)))), None),
            "the HUD reused a menu"
        );
        assert!(!Recorded::holds(Some(&hud), None), "the HUD reused the HUD");
        assert!(!Recorded::holds(None, title.as_ref()), "nothing recorded");
    }

    #[test]
    fn a_different_overlay_forces_a_rebuild() {
        let title = recorded(key(Some(Overlay::Title)));
        assert!(!Recorded::holds(
            Some(&title),
            key(Some(Overlay::Pause)).as_ref()
        ));
    }
}
