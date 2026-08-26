//! One window-system input event to one menu intent: which button a press
//! lands on, which key enters the world, which key scrolls a list. Pure
//! functions of the event and the open screen, with no window and no event
//! loop, so `state.rs` keeps only the winit shell.

use std::time::Duration;

use ox_render::{MenuButton, Overlay, menu_button};
use winit::event::MouseButton;
use winit::keyboard::{Key, NamedKey};

/// How long a click that entered the world keeps the next press out of it.
/// Longer than a double-click gap, so the second press on PLAY cannot reach
/// the game and break the block under the crosshair.
pub(crate) const ENTRY_GRACE: Duration = Duration::from_millis(300);

/// Whether a mouse press reaches the world. A press that lands within
/// [`ENTRY_GRACE`] of the click that entered it is the second half of a
/// double-click on a menu button, not an attack.
pub(crate) fn press_reaches_world(since_entry: Option<Duration>) -> bool {
    since_entry.is_none_or(|elapsed| elapsed >= ENTRY_GRACE)
}

/// The menu button a press lands on: the left button, at a cursor position
/// the window system has reported, inside one button of the column.
pub(crate) fn pressed_button(
    button: MouseButton,
    cursor: Option<[f32; 2]>,
    overlay: Overlay,
    width: f32,
    height: f32,
) -> Option<MenuButton> {
    if button != MouseButton::Left {
        return None;
    }
    let point = cursor?;
    overlay
        .buttons()
        .iter()
        .enumerate()
        .find_map(|(index, item)| {
            menu_button(width, height, overlay, index)
                .contains(point)
                .then_some(*item)
        })
}

/// Rows a key scrolls the open list by, if it scrolls at all.
pub(crate) const fn key_scroll_rows(key: &Key) -> Option<f32> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(-1.0),
        Key::Named(NamedKey::ArrowDown) => Some(1.0),
        _ => None,
    }
}

/// Keyboard route into the world, for the frames where no cursor position is
/// known and no button can be hit. Only screens offering PLAY take it.
pub(crate) fn key_starts_game(key: &Key, overlay: Overlay) -> bool {
    matches!(key, Key::Named(NamedKey::Enter | NamedKey::Space))
        && overlay.buttons().contains(&MenuButton::Play)
}

#[cfg(test)]
mod tests {
    use super::{
        Duration, ENTRY_GRACE, Key, MenuButton, MouseButton, NamedKey, Overlay, key_scroll_rows,
        key_starts_game, menu_button, press_reaches_world, pressed_button,
    };

    const SIZE: (f32, f32) = (1280.0, 720.0);

    /// Center of the `index`-th button of `overlay`'s column at [`SIZE`].
    fn center(overlay: Overlay, index: usize) -> [f32; 2] {
        let r = menu_button(SIZE.0, SIZE.1, overlay, index);
        [r.x + r.w / 2.0, r.y + r.h / 2.0]
    }

    fn press(cursor: Option<[f32; 2]>, overlay: Overlay) -> Option<MenuButton> {
        pressed_button(MouseButton::Left, cursor, overlay, SIZE.0, SIZE.1)
    }

    #[test]
    fn the_title_screen_buttons_answer_their_own_rectangles() {
        let title = Overlay::Title;
        assert_eq!(press(Some(center(title, 0)), title), Some(MenuButton::Play));
        assert_eq!(
            press(Some(center(title, 1)), title),
            Some(MenuButton::Controls)
        );
    }

    #[test]
    fn the_controls_screen_answers_back_on_its_pinned_button() {
        let controls = Overlay::Controls;
        assert_eq!(
            press(Some(center(controls, 0)), controls),
            Some(MenuButton::Back)
        );
        assert_eq!(press(Some(center(controls, 1)), controls), None);
    }

    #[test]
    fn the_back_button_sits_where_the_play_button_does_not() {
        let play = center(Overlay::Title, 0);
        assert_eq!(press(Some(play), Overlay::Controls), None);
    }

    #[test]
    fn a_press_beside_the_column_hits_nothing() {
        let r = menu_button(SIZE.0, SIZE.1, Overlay::Title, 0);
        for point in [[r.x - 1.0, r.y + 1.0], [640.0, r.y - 1.0], [640.0, 10.0]] {
            assert_eq!(press(Some(point), Overlay::Title), None, "point {point:?}");
        }
    }

    #[test]
    fn a_press_with_an_unknown_cursor_hits_nothing() {
        assert_eq!(press(None, Overlay::Title), None);
    }

    #[test]
    fn other_mouse_buttons_hit_nothing() {
        for button in [MouseButton::Right, MouseButton::Middle] {
            let point = Some(center(Overlay::Title, 0));
            let hit = pressed_button(button, point, Overlay::Title, SIZE.0, SIZE.1);
            assert_eq!(hit, None);
        }
    }

    #[test]
    fn enter_and_space_start_the_game_from_the_screens_offering_play() {
        for overlay in [Overlay::Title, Overlay::Pause] {
            assert!(key_starts_game(&Key::Named(NamedKey::Enter), overlay));
            assert!(key_starts_game(&Key::Named(NamedKey::Space), overlay));
        }
    }

    #[test]
    fn enter_does_not_start_the_game_from_the_controls_screen() {
        assert!(!key_starts_game(
            &Key::Named(NamedKey::Enter),
            Overlay::Controls
        ));
    }

    #[test]
    fn the_arrow_keys_scroll_the_open_list() {
        assert_eq!(key_scroll_rows(&Key::Named(NamedKey::ArrowUp)), Some(-1.0));
        assert_eq!(key_scroll_rows(&Key::Named(NamedKey::ArrowDown)), Some(1.0));
        assert_eq!(key_scroll_rows(&Key::Named(NamedKey::Enter)), None);
        assert_eq!(key_scroll_rows(&Key::Character("w".into())), None);
    }

    #[test]
    fn the_second_press_of_a_double_click_on_play_never_reaches_the_world() {
        assert!(!press_reaches_world(Some(Duration::ZERO)));
        assert!(!press_reaches_world(Some(
            ENTRY_GRACE.saturating_sub(Duration::from_millis(1))
        )));
    }

    #[test]
    fn a_press_reaches_the_world_once_the_grace_is_over() {
        assert!(press_reaches_world(Some(ENTRY_GRACE)));
        assert!(press_reaches_world(Some(Duration::from_secs(5))));
    }

    #[test]
    fn a_press_reaches_the_world_when_no_click_ever_entered_it() {
        assert!(press_reaches_world(None));
    }

    #[test]
    fn the_grace_outlasts_a_double_click() {
        assert!(
            ENTRY_GRACE >= Duration::from_millis(250),
            "shorter than the usual double-click gap"
        );
    }

    #[test]
    fn other_keys_never_start_the_game() {
        assert!(!key_starts_game(
            &Key::Named(NamedKey::Escape),
            Overlay::Title
        ));
        assert!(!key_starts_game(
            &Key::Character("w".into()),
            Overlay::Title
        ));
    }
}
