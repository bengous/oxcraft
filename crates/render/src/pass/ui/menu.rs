//! Menu overlay layout for the UI pass: title, splash, button column and
//! the controls screen, drawn over the dimmed world.

use super::{Overlay, UiBuilder, UiState};

/// Drop shadow behind menu text, at the caller's opacity.
const fn shadow(alpha: f32) -> [f32; 4] {
    [0.13, 0.13, 0.16, alpha]
}

/// Shadow opacity under headings and list text.
const TEXT_SHADOW_ALPHA: f32 = 0.85;
/// Shadow opacity under a label naming a control: button captions and the
/// bindings beside them.
const LABEL_SHADOW_ALPHA: f32 = 0.9;

const VEIL: [f32; 4] = [0.02, 0.04, 0.09, 0.55];
const TITLE: &str = "OXCRAFT";
const TITLE_COLOR: [f32; 4] = [0.94, 0.96, 1.0, 1.0];
const SPLASH: &str = "PUNCH TREES";
const SPLASH_COLOR: [f32; 4] = [1.0, 0.84, 0.15, 1.0];
const SPLASH_SHADOW: [f32; 4] = [0.35, 0.28, 0.0, 0.8];
const BODY: [f32; 4] = [0.43, 0.43, 0.45, 0.96];
const BODY_HOVER: [f32; 4] = [0.52, 0.56, 0.63, 1.0];
const EDGE_LIGHT: [f32; 4] = [0.66, 0.66, 0.69, 0.95];
const EDGE_DARK: [f32; 4] = [0.19, 0.19, 0.22, 0.95];
const OUTLINE: [f32; 4] = [0.07, 0.07, 0.09, 0.90];
const HEADING: &str = "CONTROLS";
const LABEL_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const CONTROLS_COLOR: [f32; 4] = [0.82, 0.86, 0.92, 0.88];
const VERSION_COLOR: [f32; 4] = [0.78, 0.78, 0.80, 0.85];

/// One row of the controls list: what it does, then how to do it.
const CONTROL_ROWS: [(&str, &str); 11] = [
    ("MOVE", "WASD"),
    ("JUMP", "SPACE"),
    ("SPRINT", "SHIFT"),
    ("TOGGLE FLY", "F"),
    ("FLY UP", "SPACE"),
    ("FLY DOWN", "SHIFT"),
    ("BREAK BLOCK", "LMB"),
    ("PLACE BLOCK", "RMB"),
    ("PICK BLOCK", "MMB"),
    ("SELECT SLOT", "1-9"),
    ("MENU", "ESC"),
];

const ROW_BG: [f32; 4] = [1.0, 1.0, 1.0, 0.05];
const TRACK_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.10];
const THUMB_COLOR: [f32; 4] = [0.82, 0.86, 0.92, 0.55];

/// Scale of the list rows and the version line.
const TEXT_SCALE: f32 = 2.0;
const CONTROLS_SHADOW: f32 = 2.0;
const CONTROLS_GAP: f32 = 40.0;
const CONTROLS_STEP: f32 = 26.0;
const ROW_PAD: f32 = 12.0;
const GLYPH_H: f32 = 8.0;
const TRACK_GAP: f32 = 8.0;
const TRACK_W: f32 = 4.0;
const THUMB_MIN: f32 = 16.0;
const VERSION_TOP: f32 = 26.0;
const HEADING_TOP: f32 = 0.20;
const LABEL_H: f32 = 24.0;
const BUTTON_TOP: f32 = 0.54;
const BUTTON_W: f32 = 400.0;
const BUTTON_H: f32 = 40.0;
const BUTTON_GAP: f32 = 12.0;
const FOOTER_GAP: f32 = 44.0;

/// One button of a menu button column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuButton {
    /// Enters the world.
    Play,
    /// Opens the controls screen.
    Controls,
    /// Leaves the current screen.
    Back,
}

impl MenuButton {
    /// Text drawn on the button.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Play => "PLAY",
            Self::Controls => "CONTROLS",
            Self::Back => "BACK",
        }
    }
}

/// Pixel rectangle used for menu hit-testing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width in pixels.
    pub w: f32,
    /// Height in pixels.
    pub h: f32,
}

impl Rect {
    /// Whether `point` lies inside the rectangle, edges included.
    #[must_use]
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.x + self.w
            && point[1] >= self.y
            && point[1] <= self.y + self.h
    }
}

/// Scroll bounds of a list: the largest offset and one row's pixel step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListScroll {
    /// Largest offset that still shows content, 0 when everything fits.
    pub max: f32,
    /// Pixels one row occupies.
    pub step: f32,
}

/// Top edge of a screen's button column. The controls screen pins its
/// column to the bottom, so the back button never scrolls away.
fn column_top(kind: Overlay, height: f32) -> f32 {
    let u = unit(height);
    match kind {
        Overlay::Controls => height - (FOOTER_GAP + BUTTON_H) * u,
        Overlay::Title | Overlay::Pause => height * BUTTON_TOP,
    }
}

fn column_width(width: f32, u: f32) -> f32 {
    (BUTTON_W * u).min(width - 16.0 * u).max(16.0 * u)
}

/// Screen rectangle of the `index`-th button of `kind`'s column, for a
/// `width` x `height` viewport; shared by drawing and click hit-testing so
/// both always agree.
#[must_use]
pub fn menu_button(width: f32, height: f32, kind: Overlay, index: usize) -> Rect {
    let u = unit(height);
    let w = column_width(width, u);
    Rect {
        x: (width - w) / 2.0,
        y: column_top(kind, height) + index as f32 * (BUTTON_H + BUTTON_GAP) * u,
        w,
        h: BUTTON_H * u,
    }
}

fn unit(height: f32) -> f32 {
    (height / 720.0).clamp(0.05, 2.0)
}

/// Rectangle the controls list scrolls inside, between the heading and the
/// pinned back button.
fn list_viewport(width: f32, height: f32) -> Rect {
    let u = unit(height);
    let y = height * HEADING_TOP + CONTROLS_GAP * u;
    let bottom = menu_button(width, height, Overlay::Controls, 0).y - CONTROLS_GAP * u;
    let w = column_width(width, u);
    Rect {
        x: (width - w) / 2.0,
        y,
        w,
        h: (bottom - y).max(0.0),
    }
}

/// Largest scroll offset that still shows content, 0 when it all fits.
fn scroll_max(content: f32, viewport: f32) -> f32 {
    (content - viewport).max(0.0)
}

/// Scroll bounds of the controls list for a `width` x `height` viewport.
#[must_use]
pub fn controls_scroll(width: f32, height: f32) -> ListScroll {
    let step = CONTROLS_STEP * unit(height);
    let content = CONTROL_ROWS.len() as f32 * step;
    ListScroll {
        max: scroll_max(content, list_viewport(width, height).h),
        step,
    }
}

/// Centered text style: fill color over a hard drop shadow.
#[derive(Clone, Copy)]
struct Style {
    scale: f32,
    offset: f32,
    color: [f32; 4],
    shadow: [f32; 4],
}

impl Style {
    fn draw(self, b: &mut UiBuilder, text: &str, cx: f32, y: f32) {
        b.text_centered(
            text,
            cx + self.offset,
            y + self.offset,
            self.scale,
            self.shadow,
        );
        b.text_centered(text, cx, y, self.scale, self.color);
    }

    /// Draws the text with its left edge at `x`.
    fn draw_at(self, b: &mut UiBuilder, text: &str, x: f32, y: f32) {
        b.text(
            text,
            x + self.offset,
            y + self.offset,
            self.scale,
            self.shadow,
        );
        b.text(text, x, y, self.scale, self.color);
    }

    const fn new(scale: f32, offset: f32, color: [f32; 4], shadow: [f32; 4]) -> Self {
        Self {
            scale,
            offset,
            color,
            shadow,
        }
    }
}

fn button(b: &mut UiBuilder, r: Rect, u: f32, hovered: bool) {
    b.rect(
        r.x - 2.0 * u,
        r.y - 2.0 * u,
        r.w + 4.0 * u,
        r.h + 4.0 * u,
        OUTLINE,
    );
    b.rect(r.x, r.y, r.w, r.h, if hovered { BODY_HOVER } else { BODY });
    let e = 2.0 * u;
    b.rect(r.x, r.y, r.w, e, EDGE_LIGHT);
    b.rect(r.x, r.y, e, r.h, EDGE_LIGHT);
    b.rect(r.x, r.y + r.h - e, r.w, e, EDGE_DARK);
    b.rect(r.x + r.w - e, r.y, e, r.h, EDGE_DARK);
}

/// Draws the scrolling rows of the controls list inside its viewport.
fn controls_list(b: &mut UiBuilder, u: f32, view: Rect, bounds: ListScroll, scroll: f32) {
    let action = Style::new(
        TEXT_SCALE * u,
        CONTROLS_SHADOW * u,
        CONTROLS_COLOR,
        shadow(TEXT_SHADOW_ALPHA),
    );
    let key = Style::new(
        TEXT_SCALE * u,
        CONTROLS_SHADOW * u,
        LABEL_COLOR,
        shadow(LABEL_SHADOW_ALPHA),
    );
    let text_dy = (bounds.step - GLYPH_H * TEXT_SCALE * u) / 2.0;
    b.clipped(view, |b| {
        for (index, (label, binding)) in CONTROL_ROWS.into_iter().enumerate() {
            let y = view.y + index as f32 * bounds.step - scroll;
            if y + bounds.step < view.y || y > view.y + view.h {
                continue;
            }
            if index % 2 == 1 {
                b.rect(view.x, y, view.w, bounds.step, ROW_BG);
            }
            action.draw_at(b, label, view.x + ROW_PAD * u, y + text_dy);
            let bw = UiBuilder::text_width(binding, TEXT_SCALE * u);
            key.draw_at(b, binding, view.x + view.w - ROW_PAD * u - bw, y + text_dy);
        }
    });
    if bounds.max <= 0.0 {
        return;
    }
    let x = view.x + view.w + TRACK_GAP * u;
    b.rect(x, view.y, TRACK_W * u, view.h, TRACK_COLOR);
    let thumb = (view.h * view.h / (view.h + bounds.max)).max(THUMB_MIN * u);
    let travel = (view.h - thumb) * (scroll / bounds.max);
    b.rect(x, view.y + travel, TRACK_W * u, thumb, THUMB_COLOR);
}

/// Draws the `kind` overlay of `ui` over a `w` x `h` viewport.
pub(super) fn draw(b: &mut UiBuilder, ui: &UiState<'_>, kind: Overlay, w: f32, h: f32) {
    let u = unit(h);
    b.rect(0.0, 0.0, w, h, VEIL);
    let cx = w / 2.0;
    let heading_y = h * HEADING_TOP;
    match kind {
        Overlay::Controls => {
            Style::new(3.0 * u, 2.0 * u, TITLE_COLOR, shadow(TEXT_SHADOW_ALPHA))
                .draw(b, HEADING, cx, heading_y);
            let bounds = controls_scroll(w, h);
            let view = list_viewport(w, h);
            controls_list(b, u, view, bounds, ui.scroll.clamp(0.0, bounds.max));
        }
        Overlay::Title | Overlay::Pause => {
            Style::new(8.0 * u, 4.0 * u, TITLE_COLOR, shadow(TEXT_SHADOW_ALPHA))
                .draw(b, TITLE, cx, heading_y);
            let splash_y = heading_y + 64.0 * u + 14.0 * u;
            Style::new(3.0 * u, 2.0 * u, SPLASH_COLOR, SPLASH_SHADOW).draw(b, SPLASH, cx, splash_y);
        }
    }
    let label = Style::new(3.0 * u, 2.0 * u, LABEL_COLOR, shadow(LABEL_SHADOW_ALPHA));
    for (index, item) in kind.buttons().iter().enumerate() {
        let r = menu_button(w, h, kind, index);
        button(b, r, u, ui.cursor.is_some_and(|p| r.contains(p)));
        label.draw(
            b,
            item.label(),
            r.x + r.w / 2.0,
            r.y + (r.h - LABEL_H * u) / 2.0,
        );
    }
    b.text(
        ui.version,
        10.0 * u,
        h - VERSION_TOP * u,
        TEXT_SCALE * u,
        VERSION_COLOR,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        CONTROL_ROWS, MenuButton, Overlay, Rect, VERSION_TOP, controls_scroll, list_viewport,
        menu_button, scroll_max, unit,
    };

    /// Viewport sizes the layout must hold, from a very short window up.
    const SIZES: [(f32, f32); 4] = [
        (640.0, 360.0),
        (1280.0, 720.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ];

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// Bottom edge of the button column of `kind`.
    fn column_bottom(width: f32, height: f32, kind: Overlay) -> f32 {
        let last = menu_button(width, height, kind, kind.buttons().len() - 1);
        last.y + last.h
    }

    #[test]
    fn the_first_button_keeps_the_classic_proportions_on_tall_viewports() {
        let r = menu_button(1280.0, 720.0, Overlay::Title, 0);
        assert!(close(r.w, 400.0) && close(r.h, 40.0));
    }

    #[test]
    fn buttons_are_horizontally_centered() {
        let (w, h) = (1280.0_f32, 720.0_f32);
        for index in 0..2 {
            let r = menu_button(w, h, Overlay::Title, index);
            assert!(close(r.x * 2.0 + r.w, w));
        }
    }

    #[test]
    fn buttons_shrink_to_fit_narrow_viewports() {
        let (w, h) = (300.0_f32, 720.0_f32);
        let r = menu_button(w, h, Overlay::Title, 0);
        assert!(close(r.w, w - 16.0));
        assert!(close(r.x, 8.0));
    }

    #[test]
    fn stacked_buttons_never_overlap() {
        for (w, h) in SIZES {
            let first = menu_button(w, h, Overlay::Title, 0);
            let second = menu_button(w, h, Overlay::Title, 1);
            assert!(
                second.y > first.y + first.h,
                "buttons overlap at height {h}"
            );
        }
    }

    #[test]
    fn the_title_and_pause_screens_open_the_controls_screen() {
        for kind in [Overlay::Title, Overlay::Pause] {
            assert_eq!(kind.buttons(), [MenuButton::Play, MenuButton::Controls]);
        }
        assert_eq!(Overlay::Controls.buttons(), [MenuButton::Back]);
    }

    #[test]
    fn the_back_button_stays_pinned_above_the_version_line() {
        for (w, h) in SIZES {
            let bottom = column_bottom(w, h, Overlay::Controls);
            assert!(
                bottom <= h - VERSION_TOP * unit(h),
                "the back button at {bottom} collides with the version line at height {h}"
            );
        }
    }

    #[test]
    fn the_list_never_covers_the_heading_or_the_back_button() {
        for (w, h) in SIZES {
            let view = list_viewport(w, h);
            let back = menu_button(w, h, Overlay::Controls, 0);
            assert!(
                view.y > h * super::HEADING_TOP,
                "the list hides the heading"
            );
            assert!(view.h > 0.0, "the list has no room at height {h}");
            assert!(
                view.y + view.h <= back.y,
                "the list runs under the back button at height {h}"
            );
        }
    }

    #[test]
    fn scrolling_to_the_end_reaches_the_last_row() {
        for (w, h) in SIZES {
            let bounds = controls_scroll(w, h);
            let view = list_viewport(w, h);
            let content = CONTROL_ROWS.len() as f32 * bounds.step;
            let last_row_bottom = content - bounds.max;
            assert!(
                last_row_bottom <= view.h + 1e-3,
                "the last row stays {last_row_bottom} below the viewport at height {h}"
            );
            assert!(bounds.step > 0.0);
        }
    }

    #[test]
    fn every_row_fits_today_so_the_list_stays_still() {
        for (w, h) in SIZES {
            let bounds = controls_scroll(w, h);
            assert!(
                close(bounds.max, 0.0),
                "the list scrolls by {} at height {h}, so a row is hidden",
                bounds.max
            );
        }
    }

    #[test]
    fn a_list_taller_than_its_viewport_scrolls_by_the_overflow() {
        assert!(close(scroll_max(300.0, 100.0), 200.0));
        assert!(close(scroll_max(100.0, 100.0), 0.0));
        assert!(close(scroll_max(40.0, 100.0), 0.0), "a short list is fixed");
    }

    #[test]
    fn the_title_screen_fits_a_short_viewport() {
        let (w, h) = (640.0_f32, 360.0_f32);
        let bottom = column_bottom(w, h, Overlay::Title);
        assert!(bottom <= h, "the column reaches {bottom} past {h}");
        assert!(
            bottom <= h - VERSION_TOP * unit(h),
            "the column at {bottom} collides with the version line"
        );
    }

    #[test]
    fn rect_contains_reports_edges_correctly() {
        let r = Rect {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        };
        assert!(r.contains([1.0, 2.0]));
        assert!(r.contains([4.0, 6.0]));
        assert!(r.contains([2.5, 4.0]));
        assert!(!r.contains([0.9, 2.0]));
        assert!(!r.contains([4.1, 3.0]));
        assert!(!r.contains([2.0, 6.1]));
    }
}
