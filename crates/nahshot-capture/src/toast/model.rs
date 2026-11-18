//! Pure toast logic: texts, colors, placement and the animation timeline.

use crate::geom::PxRect;
use nahshot_i18n::t;
use std::time::Duration;
use tiny_skia::Color;

/// What happened to the capture, which picks the toast's subtitle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Copied to the clipboard and saved to the gallery.
    Copied,
    /// Saved to the gallery only.
    Saved,
}

impl ToastKind {
    /// The heading, in the current language.
    pub fn title(self) -> String {
        t("toast.title")
    }

    /// The line under the heading, in the current language.
    pub fn subtitle(self) -> String {
        match self {
            ToastKind::Copied => t("toast.copied"),
            ToastKind::Saved => t("toast.saved"),
        }
    }
}

/// The toast's colors for one theme (same values as `ui/src/theme.css`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub card: Color,
    pub border: Color,
    pub title: Color,
    pub subtitle: Color,
    pub accent: Color,
    pub check_bg: Color,
    pub shadow: Color,
}

pub fn palette(dark: bool) -> Palette {
    let rgb = |r, g, b| Color::from_rgba8(r, g, b, 255);
    let alpha = |c: Color, a: f32| {
        let mut c = c;
        c.set_alpha(a);
        c
    };
    if dark {
        let accent = rgb(0x6e, 0x8b, 0xff);
        Palette {
            card: rgb(0x2a, 0x2b, 0x30),
            border: rgb(0x3a, 0x3c, 0x43),
            title: rgb(0xec, 0xec, 0xef),
            subtitle: rgb(0xa4, 0xa7, 0xb0),
            accent,
            check_bg: alpha(accent, 0.20),
            shadow: Color::from_rgba(0.0, 0.0, 0.0, 0.45).expect("valid color"),
        }
    } else {
        let accent = rgb(0x4a, 0x6c, 0xf7);
        Palette {
            card: rgb(0xff, 0xff, 0xff),
            border: rgb(0xe3, 0xe4, 0xe8),
            title: rgb(0x1a, 0x1b, 0x1e),
            subtitle: rgb(0x5d, 0x60, 0x69),
            accent,
            check_bg: alpha(accent, 0.14),
            shadow: Color::from_rgba(0.0, 0.0, 0.0, 0.18).expect("valid color"),
        }
    }
}

/// Card size, in logical pixels.
pub const CARD_W: f32 = 260.0;
pub const CARD_H: f32 = 64.0;
/// Gap between the card and the work area's right and bottom edges.
pub const MARGIN: f32 = 16.0;
/// Transparent room around the card for its shadow.
pub const SHADOW: f32 = 16.0;
/// How far right the card starts before sliding in.
pub const SLIDE: f32 = 24.0;

/// `v` logical pixels at `scale`, rounded to physical pixels.
pub fn px(v: f32, scale: f32) -> i32 {
    (v * scale).round() as i32
}

/// The toast window's rectangle (card plus shadow room) in the bottom-right corner
/// of `work`, a monitor's work area in physical pixels.
pub fn layout(work: PxRect, scale: f32) -> PxRect {
    let shadow = px(SHADOW, scale);
    let (w, h) = (
        px(CARD_W, scale) + 2 * shadow,
        px(CARD_H, scale) + 2 * shadow,
    );
    let margin = px(MARGIN, scale);
    PxRect {
        x: work.x + work.w as i32 - margin + shadow - w,
        y: work.y + work.h as i32 - margin + shadow - h,
        w: w as u32,
        h: h as u32,
    }
}

/// Slide and fade in.
pub const ENTER: Duration = Duration::from_millis(250);
/// Fully shown while the bar drains.
pub const HOLD: Duration = Duration::from_millis(2300);
/// Fade out.
pub const EXIT: Duration = Duration::from_millis(300);

/// One animation frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// 0 (invisible) to 1.
    pub opacity: f32,
    /// Logical pixels right of the resting position.
    pub offset_x: f32,
    /// Remaining fraction of the time bar, 1 to 0.
    pub bar: f32,
}

/// The frame `elapsed` after the toast appeared; `None` once it is over.
pub fn frame_at(elapsed: Duration) -> Option<Frame> {
    let ratio = |t: Duration, of: Duration| (t.as_secs_f32() / of.as_secs_f32()).clamp(0.0, 1.0);
    if elapsed < ENTER {
        let p = 1.0 - (1.0 - ratio(elapsed, ENTER)).powi(3);
        return Some(Frame {
            opacity: p,
            offset_x: SLIDE * (1.0 - p),
            bar: 1.0,
        });
    }
    let held = elapsed - ENTER;
    if held < HOLD {
        return Some(Frame {
            opacity: 1.0,
            offset_x: 0.0,
            bar: 1.0 - ratio(held, HOLD),
        });
    }
    let leaving = held - HOLD;
    (leaving < EXIT).then(|| Frame {
        opacity: 1.0 - ratio(leaving, EXIT),
        offset_x: 0.0,
        bar: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts() {
        // Tests never change the language, so it is the default, English.
        assert_eq!(ToastKind::Copied.title(), "Screenshot saved");
        assert_eq!(ToastKind::Saved.title(), "Screenshot saved");
        assert_eq!(ToastKind::Copied.subtitle(), "Copied · it's in the gallery");
        assert_eq!(ToastKind::Saved.subtitle(), "It's in the nahshot gallery");
    }

    #[test]
    fn layout_bottom_right_at_100() {
        let w = layout(
            PxRect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            },
            1.0,
        );
        assert_eq!((w.w, w.h), (260 + 32, 64 + 32));
        // The card sits 16 px from the work area's edges; its shadow margin reaches them.
        assert_eq!(w.x + w.w as i32, 1920);
        assert_eq!(w.y + w.h as i32, 1040);
    }

    #[test]
    fn layout_scales() {
        let w = layout(
            PxRect {
                x: 0,
                y: 0,
                w: 2880,
                h: 1560,
            },
            1.5,
        );
        assert_eq!((w.w, w.h), (438, 144));
        assert_eq!(w.x + w.w as i32, 2880);
        assert_eq!(w.y + w.h as i32, 1560);
    }

    #[test]
    fn layout_respects_work_area_offset() {
        // Taskbar on the left of a monitor placed left of the primary (negative x),
        // and a taskbar on top (work area starts at y = 40).
        let work = PxRect {
            x: -1920 + 48,
            y: 40,
            w: 1920 - 48,
            h: 1040,
        };
        let w = layout(work, 1.0);
        assert_eq!(w.x + w.w as i32, 0);
        assert_eq!(w.y + w.h as i32, 1080);
        assert!(w.x >= work.x);
    }

    #[test]
    fn timeline() {
        let a = frame_at(Duration::ZERO).unwrap();
        assert_eq!((a.opacity, a.offset_x, a.bar), (0.0, SLIDE, 1.0));
        let b = frame_at(ENTER).unwrap();
        assert_eq!((b.opacity, b.offset_x, b.bar), (1.0, 0.0, 1.0));
        let c = frame_at(ENTER + HOLD / 2).unwrap();
        assert!((c.bar - 0.5).abs() < 0.01 && c.opacity == 1.0 && c.offset_x == 0.0);
        let d = frame_at(ENTER + HOLD + EXIT / 2).unwrap();
        assert!(d.opacity > 0.0 && d.opacity < 1.0 && d.bar == 0.0);
        assert!(frame_at(ENTER + HOLD + EXIT).is_none());
    }

    #[test]
    fn palettes_differ() {
        assert_ne!(palette(false).card, palette(true).card);
        assert_ne!(palette(false).title, palette(true).title);
    }
}
