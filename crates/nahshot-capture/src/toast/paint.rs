//! Draws one toast frame into a pixmap.

use crate::annotate::Color as TextColor;
use crate::font::Font;
use crate::render::draw_text;
use crate::toast::model::{px, Frame, Palette, ToastKind, CARD_H, CARD_W, SHADOW};
use crate::CaptureError;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke,
    Transform,
};

/// Corner radius of the card.
const RADIUS: f32 = 10.0;
/// How far below the card its shadow falls.
const SHADOW_DROP: f32 = 4.0;
/// Box-blur radius of the shadow (three passes approximate a Gaussian).
const BLUR: f32 = 5.0;
/// Check circle: diameter and gap from the card's left edge.
const CHECK_D: f32 = 28.0;
const CHECK_LEFT: f32 = 14.0;
/// Where the text block starts, from the card's left edge.
const TEXT_LEFT: f32 = 54.0;
const TITLE_SIZE: f32 = 14.0;
const TITLE_TOP: f32 = 11.0;
const SUBTITLE_SIZE: f32 = 12.0;
const SUBTITLE_TOP: f32 = 32.0;
/// Height of the time bar at the card's bottom.
const BAR_H: f32 = 3.0;

/// The faces the toast draws with.
pub(crate) struct Fonts {
    pub regular: Font,
    pub bold: Font,
}

impl Fonts {
    pub fn load() -> Result<Fonts, CaptureError> {
        Ok(Fonts {
            regular: Font::system()?,
            bold: Font::system_bold()?,
        })
    }
}

/// Draws the toast window's content (card, shadow room included) for `frame`, at
/// `scale` physical pixels per logical one. Pixels are premultiplied RGBA. The slide
/// is not drawn: the window itself moves.
pub(crate) fn paint(
    kind: ToastKind,
    pal: &Palette,
    scale: f32,
    frame: &Frame,
    fonts: &Fonts,
) -> Pixmap {
    let s = scale;
    let sh = px(SHADOW, s) as f32;
    let (cw, ch) = (px(CARD_W, s) as f32, px(CARD_H, s) as f32);
    let (w, h) = ((cw + 2.0 * sh) as u32, (ch + 2.0 * sh) as u32);
    let mut out = Pixmap::new(w, h).expect("toast size is never zero");
    let card = Rect::from_xywh(sh, sh, cw, ch).expect("valid card rect");
    let card_path = rounded(card, RADIUS * s);
    let id = Transform::identity();

    // Shadow: the card's shape, dropped a little and blurred.
    let drop = Transform::from_translate(0.0, SHADOW_DROP * s);
    out.fill_path(
        &card_path,
        &solid(pal.shadow),
        FillRule::Winding,
        drop,
        None,
    );
    blur_alpha(&mut out, (BLUR * s).round().max(1.0) as usize);

    // Card and border.
    out.fill_path(&card_path, &solid(pal.card), FillRule::Winding, id, None);
    let inset = Rect::from_xywh(sh + 0.5, sh + 0.5, cw - 1.0, ch - 1.0).expect("valid rect");
    let border = Stroke {
        width: 1.0,
        ..Stroke::default()
    };
    let border_path = rounded(inset, RADIUS * s - 0.5);
    out.stroke_path(&border_path, &solid(pal.border), &border, id, None);

    // Check mark in a soft circle.
    let r = CHECK_D * s / 2.0;
    let (cx, cy) = (card.x() + CHECK_LEFT * s + r, card.y() + ch / 2.0);
    if let Some(circle) = PathBuilder::from_circle(cx, cy, r) {
        out.fill_path(&circle, &solid(pal.check_bg), FillRule::Winding, id, None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 5.5 * s, cy + 0.5 * s);
    pb.line_to(cx - 1.5 * s, cy + 4.5 * s);
    pb.line_to(cx + 6.0 * s, cy - 4.0 * s);
    if let Some(tick) = pb.finish() {
        let stroke = Stroke {
            width: 2.5 * s,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        out.stroke_path(&tick, &solid(pal.accent), &stroke, id, None);
    }

    // Texts.
    let x = card.x() + TEXT_LEFT * s;
    let mut canvas = out.as_mut();
    let title_at = (x, card.y() + TITLE_TOP * s);
    let sub_at = (x, card.y() + SUBTITLE_TOP * s);
    draw_text(
        &mut canvas,
        title_at,
        kind.title(),
        text(pal.title),
        TITLE_SIZE * s,
        &fonts.bold,
    );
    let sub_size = SUBTITLE_SIZE * s;
    draw_text(
        &mut canvas,
        sub_at,
        kind.subtitle(),
        text(pal.subtitle),
        sub_size,
        &fonts.regular,
    );

    // Time bar, clipped to the card's rounded bottom.
    let bar_w = (cw * frame.bar.clamp(0.0, 1.0)).round();
    if bar_w > 0.0 {
        let bar_h = (BAR_H * s).round();
        let mut mask = Mask::new(w, h).expect("toast size is never zero");
        mask.fill_path(&card_path, FillRule::Winding, true, id);
        let bar =
            Rect::from_xywh(card.x(), card.bottom() - bar_h, bar_w, bar_h).expect("valid bar");
        out.fill_rect(bar, &solid(pal.accent), id, Some(&mask));
    }

    fade(&mut out, frame.opacity);
    out
}

fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn text(c: Color) -> TextColor {
    let c = c.to_color_u8();
    TextColor(c.red(), c.green(), c.blue())
}

/// A rectangle with corners of radius `r`.
fn rounded(rect: Rect, r: f32) -> Path {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let (l, t, rt, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    // Control-point distance for a quarter circle drawn with one cubic.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rt - r, t);
    pb.cubic_to(rt - r + k, t, rt, t + r - k, rt, t + r);
    pb.line_to(rt, b - r);
    pb.cubic_to(rt, b - r + k, rt - r + k, b, rt - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - k, b, l, b - r + k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - k, l + r - k, t, l + r, t);
    pb.close();
    pb.finish().expect("rounded rect path")
}

/// Three box-blur passes over a pixmap that holds only black (shadow) pixels.
fn blur_alpha(p: &mut Pixmap, radius: usize) {
    let (w, h) = (p.width() as usize, p.height() as usize);
    let mut a: Vec<f32> = p.data().chunks_exact(4).map(|px| px[3] as f32).collect();
    let mut tmp = vec![0.0; a.len()];
    for _ in 0..3 {
        box_pass(&a, &mut tmp, w, h, radius, true);
        box_pass(&tmp, &mut a, w, h, radius, false);
    }
    for (px, v) in p.data_mut().chunks_exact_mut(4).zip(a) {
        px.copy_from_slice(&[0, 0, 0, v.round().clamp(0.0, 255.0) as u8]);
    }
}

/// One box blur along rows (`horizontal`) or columns; outside the image counts as empty.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum: f32 = (0..=r.min(len - 1)).map(|i| src[at(line, i)]).sum();
        for i in 0..len {
            dst[at(line, i)] = sum * norm;
            if i + r + 1 < len {
                sum += src[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= src[at(line, i - r)];
            }
        }
    }
}

/// Multiplies every premultiplied channel by `opacity`.
fn fade(p: &mut Pixmap, opacity: f32) {
    if opacity >= 1.0 {
        return;
    }
    let o = opacity.clamp(0.0, 1.0);
    for b in p.data_mut() {
        *b = (*b as f32 * o).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toast::model::{palette, Frame, ToastKind};
    use tiny_skia::Pixmap;

    fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = p.pixel(x, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    fn full() -> Frame {
        Frame {
            opacity: 1.0,
            offset_x: 0.0,
            bar: 1.0,
        }
    }

    fn fonts() -> Fonts {
        Fonts::load().expect("system fonts")
    }

    #[test]
    fn size_matches_layout() {
        let p = paint(ToastKind::Saved, &palette(false), 1.0, &full(), &fonts());
        assert_eq!((p.width(), p.height()), (292, 96));
        let p = paint(ToastKind::Saved, &palette(false), 1.5, &full(), &fonts());
        assert_eq!((p.width(), p.height()), (438, 144));
    }

    #[test]
    fn corner_transparent_center_opaque() {
        let p = paint(ToastKind::Saved, &palette(false), 1.0, &full(), &fonts());
        assert!(px(&p, 0, 0)[3] < 8, "only shadow residue in the corner");
        assert_eq!(px(&p, 16 + 200, 16 + 10)[3], 255);
    }

    #[test]
    fn card_color_is_the_palette_card() {
        let pal = palette(true);
        let p = paint(ToastKind::Saved, &pal, 1.0, &full(), &fonts());
        let c = pal.card.to_color_u8();
        assert_eq!(
            px(&p, 16 + 250, 16 + 10),
            [c.red(), c.green(), c.blue(), 255]
        );
    }

    #[test]
    fn bar_follows_fraction() {
        let pal = palette(false);
        let half = paint(
            ToastKind::Saved,
            &pal,
            1.0,
            &Frame { bar: 0.5, ..full() },
            &fonts(),
        );
        let y = 16 + 64 - 2;
        let a = pal.accent.to_color_u8();
        let accent = [a.red(), a.green(), a.blue()];
        assert_eq!(px(&half, 16 + 40, y)[..3], accent);
        assert_ne!(px(&half, 16 + 220, y)[..3], accent);
    }

    #[test]
    fn opacity_scales_alpha() {
        let p = paint(
            ToastKind::Saved,
            &palette(true),
            1.0,
            &Frame {
                opacity: 0.5,
                ..full()
            },
            &fonts(),
        );
        let a = px(&p, 16 + 200, 16 + 10)[3];
        assert!((120..=135).contains(&a), "alpha {a}");
    }

    #[test]
    fn text_is_drawn() {
        let pal = palette(false);
        let p = paint(ToastKind::Saved, &pal, 1.0, &full(), &fonts());
        let c = pal.card.to_color_u8();
        let card = [c.red(), c.green(), c.blue(), 255];
        // The text block starts 54 px into the card; some pixel there is not plain card.
        let inked = (16 + 54..16 + 240)
            .flat_map(|x| (16 + 12..16 + 52).map(move |y| (x, y)))
            .any(|(x, y)| px(&p, x, y) != card);
        assert!(inked);
    }

    /// Writes target/toast-light.png and target/toast-dark.png to look at.
    #[test]
    #[ignore]
    fn dump_png() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for (name, dark, kind) in [
            ("light", false, ToastKind::Copied),
            ("dark", true, ToastKind::Saved),
        ] {
            let f = Frame { bar: 0.6, ..full() };
            let p = paint(kind, &palette(dark), 2.0, &f, &fonts());
            p.save_png(dir.join(format!("toast-{name}.png"))).unwrap();
        }
    }
}
