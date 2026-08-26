//! Menu navigation: the stack of open screens over the running world.

use ox_render::Overlay;

/// One menu the player can open. The world keeps running underneath.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Screen {
    /// Start screen, the root of the stack before the first entry.
    Title,
    /// Pause screen of a run already started.
    Pause,
    /// Controls screen, opened from the title or pause screen. It carries
    /// its own scroll offset, so reopening it starts from the top.
    Controls {
        /// Pixels the list is scrolled down by.
        scroll: f32,
    },
}

impl Screen {
    /// The controls screen, scrolled to the top.
    pub(crate) const fn controls() -> Self {
        Self::Controls { scroll: 0.0 }
    }
}

/// Open menus, innermost last. An empty stack gives the world focus.
pub(crate) struct Screens {
    stack: Vec<Screen>,
}

impl Screens {
    /// A stack showing the title screen.
    pub(crate) fn new() -> Self {
        Self {
            stack: vec![Screen::Title],
        }
    }

    /// The screen the player interacts with, or `None` in the world.
    pub(crate) fn top(&self) -> Option<Screen> {
        self.stack.last().copied()
    }

    /// Whether the world has focus with no menu over it.
    pub(crate) const fn playing(&self) -> bool {
        self.stack.is_empty()
    }

    /// Overlay the renderer draws for the current screen, or `None` in the
    /// world, where it draws the HUD.
    pub(crate) fn overlay(&self) -> Option<Overlay> {
        Some(match self.top()? {
            Screen::Title => Overlay::Title,
            Screen::Pause => Overlay::Pause,
            Screen::Controls { .. } => Overlay::Controls,
        })
    }

    /// Pixels the open list is scrolled down by.
    pub(crate) fn scroll(&self) -> f32 {
        match self.top() {
            Some(Screen::Controls { scroll }) => scroll,
            None | Some(Screen::Title | Screen::Pause) => 0.0,
        }
    }

    /// Scrolls the open list by `delta` pixels, kept inside `[0, max]`.
    pub(crate) fn scroll_by(&mut self, delta: f32, max: f32) {
        if let Some(Screen::Controls { scroll }) = self.stack.last_mut() {
            *scroll = (*scroll + delta).clamp(0.0, max.max(0.0));
        }
    }

    /// Opens `screen` over the current one.
    pub(crate) fn open(&mut self, screen: Screen) {
        self.stack.push(screen);
    }

    /// Leaves every menu and gives the world focus.
    pub(crate) fn enter_world(&mut self) {
        self.stack.clear();
    }

    /// Applies ESC: opens the pause menu from the world, or leaves the top
    /// menu. The title screen is the root of the stack and stays.
    pub(crate) fn escape(&mut self) {
        match self.top() {
            None => self.stack.push(Screen::Pause),
            Some(Screen::Title) => {}
            Some(Screen::Pause | Screen::Controls { .. }) => {
                self.stack.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Overlay, Screen, Screens};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_new_stack_shows_the_title_screen() {
        let screens = Screens::new();
        assert_eq!(screens.top(), Some(Screen::Title));
        assert_eq!(screens.overlay(), Some(Overlay::Title));
        assert!(!screens.playing());
    }

    #[test]
    fn entering_the_world_empties_the_stack() {
        let mut screens = Screens::new();
        screens.enter_world();
        assert_eq!(screens.top(), None);
        assert_eq!(screens.overlay(), None);
        assert!(screens.playing());
    }

    #[test]
    fn escape_opens_the_pause_menu_over_the_world() {
        let mut screens = Screens::new();
        screens.enter_world();
        screens.escape();
        assert_eq!(screens.top(), Some(Screen::Pause));
        assert_eq!(screens.overlay(), Some(Overlay::Pause));
        assert!(!screens.playing());
    }

    #[test]
    fn escape_leaves_the_pause_menu_back_to_the_world() {
        let mut screens = Screens::new();
        screens.enter_world();
        screens.escape();
        screens.escape();
        assert!(screens.playing());
    }

    #[test]
    fn escape_keeps_the_title_screen_up() {
        let mut screens = Screens::new();
        for _ in 0..3 {
            screens.escape();
        }
        assert_eq!(screens.top(), Some(Screen::Title));
        assert!(!screens.playing());
    }

    #[test]
    fn the_controls_screen_returns_to_the_title_screen() {
        let mut screens = Screens::new();
        screens.open(Screen::controls());
        assert_eq!(screens.overlay(), Some(Overlay::Controls));
        screens.escape();
        assert_eq!(screens.top(), Some(Screen::Title));
    }

    #[test]
    fn the_controls_screen_returns_to_the_pause_screen() {
        let mut screens = Screens::new();
        screens.enter_world();
        screens.escape();
        screens.open(Screen::controls());
        assert_eq!(screens.overlay(), Some(Overlay::Controls));
        screens.escape();
        assert_eq!(screens.top(), Some(Screen::Pause));
        assert!(!screens.playing());
    }

    #[test]
    fn the_list_scroll_stays_inside_its_bounds() {
        let mut screens = Screens::new();
        screens.open(Screen::controls());
        screens.scroll_by(-50.0, 120.0);
        assert!(close(screens.scroll(), 0.0), "scrolled above the first row");
        screens.scroll_by(500.0, 120.0);
        assert!(close(screens.scroll(), 120.0), "scrolled past the last row");
        screens.scroll_by(-30.0, 120.0);
        assert!(close(screens.scroll(), 90.0));
    }

    #[test]
    fn a_list_that_fits_never_scrolls() {
        let mut screens = Screens::new();
        screens.open(Screen::controls());
        screens.scroll_by(200.0, 0.0);
        assert!(close(screens.scroll(), 0.0));
    }

    #[test]
    fn screens_without_a_list_ignore_scrolling() {
        let mut screens = Screens::new();
        screens.scroll_by(100.0, 500.0);
        assert!(close(screens.scroll(), 0.0), "the title screen scrolled");
        screens.enter_world();
        screens.scroll_by(100.0, 500.0);
        assert!(close(screens.scroll(), 0.0), "the world scrolled");
    }

    #[test]
    fn reopening_the_controls_screen_starts_at_the_top() {
        let mut screens = Screens::new();
        screens.open(Screen::controls());
        screens.scroll_by(90.0, 120.0);
        screens.escape();
        screens.open(Screen::controls());
        assert!(close(screens.scroll(), 0.0));
    }

    #[test]
    fn the_title_screen_never_comes_back_after_the_first_entry() {
        let mut screens = Screens::new();
        screens.enter_world();
        screens.escape();
        assert_eq!(screens.overlay(), Some(Overlay::Pause));
        screens.enter_world();
        screens.escape();
        assert_eq!(screens.overlay(), Some(Overlay::Pause));
    }
}
