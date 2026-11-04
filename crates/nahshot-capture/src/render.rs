//! Desktop composition, cropping and annotation rasterization.

use crate::annotate::{Annotations, Color, Pt, Shape, MARKER_ALPHA, MARKER_WIDTH};
use crate::font::Font;
use crate::geom::{PxPoint, PxRect};
use image::{Rgba, RgbaImage};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

/// One monitor's capture; origin and size are in physical pixels.
pub struct MonitorShot {
    pub id: u32,
    pub origin: PxPoint,
    pub scale: f32,
    pub image: RgbaImage,
}

/// All monitors stitched into the virtual desktop.
pub struct Desktop {
    pub origin: PxPoint,
    pub image: RgbaImage,
}

/// Stitches the shots into their bounding box; areas without a monitor are black.
pub fn compose(shots: &[MonitorShot]) -> Desktop {
    let bounds = shots
        .iter()
        .map(|s| {
            (
                s.origin.x,
                s.origin.y,
                s.origin.x + s.image.width() as i32,
                s.origin.y + s.image.height() as i32,
            )
        })
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)));
    let Some((l, t, r, b)) = bounds else {
        return Desktop {
            origin: PxPoint { x: 0, y: 0 },
            image: RgbaImage::new(0, 0),
        };
    };
    let mut image = RgbaImage::from_pixel((r - l) as u32, (b - t) as u32, Rgba([0, 0, 0, 255]));
    for s in shots {
        let (x, y) = ((s.origin.x - l) as i64, (s.origin.y - t) as i64);
        image::imageops::replace(&mut image, &s.image, x, y);
    }
    Desktop {
        origin: PxPoint { x: l, y: t },
        image,
    }
}

/// Cuts `r` (virtual-desktop coordinates) out of the desktop; any part outside is dropped.
pub fn crop(desktop: &Desktop, r: PxRect) -> RgbaImage {
    let bounds = PxRect {
        x: desktop.origin.x,
        y: desktop.origin.y,
        w: desktop.image.width(),
        h: desktop.image.height(),
    };
    match r.intersect(&bounds) {
        Some(i) => image::imageops::crop_imm(
            &desktop.image,
            (i.x - bounds.x) as u32,
            (i.y - bounds.y) as u32,
            i.w,
            i.h,
        )
        .to_image(),
        None => RgbaImage::new(0, 0),
    }
}

/// Returns a copy of `base` with the annotations drawn on top.
pub fn flatten(base: &RgbaImage, ann: &Annotations, font: &Font) -> RgbaImage {
    if ann.is_empty() || base.width() == 0 || base.height() == 0 {
        return base.clone();
    }
    let mut data = base.as_raw().clone();
    premultiply(&mut data);
    if let Some(mut pixmap) = PixmapMut::from_bytes(&mut data, base.width(), base.height()) {
        draw_shapes(&mut pixmap, (0.0, 0.0), ann, font);
    }
    demultiply(&mut data);
    RgbaImage::from_raw(base.width(), base.height(), data).expect("buffer size is unchanged")
}

/// Draws the shapes into `pixmap`, shifted by `offset`.
pub fn draw_shapes(pixmap: &mut PixmapMut, offset: (f32, f32), ann: &Annotations, font: &Font) {
    let tf = Transform::from_translate(offset.0, offset.1);
    for shape in ann.shapes() {
        match shape {
            Shape::Pen { pts, color, width } => {
                stroke_polyline(pixmap, pts, paint(*color, 1.0), *width, tf);
            }
            Shape::Line { a, b, color, width } => {
                stroke_polyline(pixmap, &[*a, *b], paint(*color, 1.0), *width, tf);
            }
            Shape::Arrow { a, b, color, width } => draw_arrow(pixmap, *a, *b, *color, *width, tf),
            Shape::Rect { a, b, color, width } => {
                if let Some(r) = rect_of(*a, *b) {
                    let path = PathBuilder::from_rect(r);
                    stroke_path(pixmap, &path, paint(*color, 1.0), *width, tf);
                }
            }
            Shape::Ellipse { a, b, color, width } => {
                if let Some(path) = rect_of(*a, *b).and_then(PathBuilder::from_oval) {
                    stroke_path(pixmap, &path, paint(*color, 1.0), *width, tf);
                }
            }
            Shape::Marker { pts, color } => {
                stroke_polyline(pixmap, pts, paint(*color, MARKER_ALPHA), MARKER_WIDTH, tf);
            }
            Shape::Text {
                at,
                text,
                color,
                size,
            } => {
                let top_left = (at.0 + offset.0, at.1 + offset.1);
                draw_text(pixmap, top_left, text, *color, *size, font);
            }
        }
    }
}

fn paint(c: Color, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, (alpha * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

fn rect_of(a: Pt, b: Pt) -> Option<Rect> {
    Rect::from_ltrb(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

fn stroke_path(pixmap: &mut PixmapMut, path: &Path, paint: Paint, width: f32, tf: Transform) {
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, tf, None);
}

/// Strokes a round-capped polyline; a single point becomes a dot.
fn stroke_polyline(pixmap: &mut PixmapMut, pts: &[Pt], paint: Paint, width: f32, tf: Transform) {
    let mut pb = PathBuilder::new();
    match pts {
        [] => return,
        [p] => {
            pb.push_circle(p.0, p.1, width / 2.0);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, tf, None);
            }
            return;
        }
        [first, rest @ ..] => {
            pb.move_to(first.0, first.1);
            for p in rest {
                pb.line_to(p.0, p.1);
            }
        }
    }
    if let Some(path) = pb.finish() {
        stroke_path(pixmap, &path, paint, width, tf);
    }
}

/// Shaft with round cap plus a filled equilateral head (side = 4 x width) at `b`.
fn draw_arrow(pixmap: &mut PixmapMut, a: Pt, b: Pt, color: Color, width: f32, tf: Transform) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let side = 4.0 * width;
    let back = side * 3f32.sqrt() / 2.0;
    let base = (b.0 - ux * back, b.1 - uy * back);
    let (nx, ny) = (-uy * side / 2.0, ux * side / 2.0);

    if len > back {
        stroke_polyline(pixmap, &[a, base], paint(color, 1.0), width, tf);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(b.0, b.1);
    pb.line_to(base.0 + nx, base.1 + ny);
    pb.line_to(base.0 - nx, base.1 - ny);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint(color, 1.0), FillRule::Winding, tf, None);
    }
}

/// Draws `text` with its top-left at `at`, compositing glyph coverage directly.
fn draw_text(pixmap: &mut PixmapMut, at: Pt, text: &str, color: Color, size: f32, font: &Font) {
    let font = font.raw();
    let ascent = font
        .horizontal_line_metrics(size)
        .map_or(size, |m| m.ascent);
    let baseline = at.1 + ascent;
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let mut pen = at.0;
    for ch in text.chars() {
        let (m, coverage) = font.rasterize(ch, size);
        let gx = (pen + m.xmin as f32).round() as i32;
        let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
        for (i, &cov) in coverage.iter().enumerate() {
            let (x, y) = (gx + (i % m.width) as i32, gy + (i / m.width) as i32);
            if cov == 0 || x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            blend(pixmap.data_mut(), ((y * w + x) * 4) as usize, color, cov);
        }
        pen += m.advance_width;
    }
}

/// Source-over of `color` at coverage `cov` onto a premultiplied RGBA pixel.
fn blend(data: &mut [u8], idx: usize, color: Color, cov: u8) {
    let a = cov as u32;
    let mix = |dst: u8, src: u8| ((src as u32 * a + dst as u32 * (255 - a) + 127) / 255) as u8;
    data[idx] = mix(data[idx], color.0);
    data[idx + 1] = mix(data[idx + 1], color.1);
    data[idx + 2] = mix(data[idx + 2], color.2);
    data[idx + 3] = mix(data[idx + 3], 255);
}

fn premultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
}

fn demultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = ((*c as u32 * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::{PALETTE, STROKE_WIDTH};
    use image::Rgba;

    const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
    const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);
    const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);
    const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

    fn shot(id: u32, x: i32, y: i32, w: u32, h: u32, scale: f32, c: Rgba<u8>) -> MonitorShot {
        MonitorShot {
            id,
            origin: PxPoint { x, y },
            scale,
            image: RgbaImage::from_pixel(w, h, c),
        }
    }

    fn two_monitors() -> Desktop {
        compose(&[
            shot(1, -1920, 0, 1920, 1080, 1.0, RED),
            shot(2, 0, 0, 2560, 1440, 1.5, BLUE),
        ])
    }

    fn font() -> Font {
        Font::system().expect("system font")
    }

    fn white(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, WHITE)
    }

    #[test]
    fn compose_two_monitors_with_negative_origin() {
        let d = two_monitors();
        assert_eq!(d.origin, PxPoint { x: -1920, y: 0 });
        assert_eq!(d.image.dimensions(), (4480, 1440));
        assert_eq!(*d.image.get_pixel(0, 0), RED);
        assert_eq!(*d.image.get_pixel(1920, 0), BLUE);
        assert_eq!(*d.image.get_pixel(0, 1200), BLACK);
    }

    #[test]
    fn crop_across_monitors() {
        let d = two_monitors();
        let c = crop(
            &d,
            PxRect {
                x: -10,
                y: 0,
                w: 20,
                h: 10,
            },
        );
        assert_eq!(c.dimensions(), (20, 10));
        for x in 0..10 {
            assert_eq!(*c.get_pixel(x, 5), RED);
        }
        for x in 10..20 {
            assert_eq!(*c.get_pixel(x, 5), BLUE);
        }
    }

    #[test]
    fn crop_drops_part_outside_desktop() {
        let d = two_monitors();
        let c = crop(
            &d,
            PxRect {
                x: 2550,
                y: 1430,
                w: 50,
                h: 50,
            },
        );
        assert_eq!(c.dimensions(), (10, 10));
    }

    #[test]
    fn flatten_draws_rect_outline_in_color() {
        let mut ann = Annotations::default();
        ann.push(Shape::Rect {
            a: (10.0, 10.0),
            b: (90.0, 90.0),
            color: PALETTE[0],
            width: 3.0,
        });
        let out = flatten(&white(100, 100), &ann, &font());
        assert_eq!(*out.get_pixel(10, 50), Rgba([0xE5, 0x39, 0x35, 255]));
        assert_eq!(*out.get_pixel(50, 50), WHITE);
    }

    #[test]
    fn flatten_empty_annotations_is_identity() {
        let mut base = white(30, 20);
        base.put_pixel(3, 4, Rgba([10, 20, 30, 200]));
        let out = flatten(&base, &Annotations::default(), &font());
        assert_eq!(out.as_raw(), base.as_raw());
    }

    #[test]
    fn flatten_arrow_has_head_at_tip() {
        let mut ann = Annotations::default();
        ann.push(Shape::Arrow {
            a: (10.0, 50.0),
            b: (90.0, 50.0),
            color: PALETTE[0],
            width: 3.0,
        });
        let out = flatten(&white(100, 100), &ann, &font());
        // Head (side 12) is wide near the tip; the shaft alone is only 3px thick.
        assert_eq!(*out.get_pixel(85, 48), Rgba([0xE5, 0x39, 0x35, 255]));
        assert_eq!(*out.get_pixel(50, 46), WHITE);
    }

    #[test]
    fn flatten_ellipse_marks_axis_extremes_not_center() {
        let mut ann = Annotations::default();
        ann.push(Shape::Ellipse {
            a: (10.0, 10.0),
            b: (90.0, 90.0),
            color: PALETTE[4],
            width: STROKE_WIDTH,
        });
        let out = flatten(&white(100, 100), &ann, &font());
        assert_eq!(*out.get_pixel(10, 50), Rgba([0x1E, 0x88, 0xE5, 255]));
        assert_eq!(*out.get_pixel(50, 50), WHITE);
    }

    #[test]
    fn flatten_marker_blends_with_alpha() {
        let mut ann = Annotations::default();
        ann.push(Shape::Marker {
            pts: vec![(10.0, 50.0), (90.0, 50.0)],
            color: PALETTE[0],
        });
        let out = flatten(&white(100, 100), &ann, &font());
        let p = out.get_pixel(50, 50);
        // 0.35 * (229,57,53) + 0.65 * 255 = (246,186,184).
        assert!((p[0] as i32 - 246).abs() <= 2, "{p:?}");
        assert!((p[1] as i32 - 186).abs() <= 2, "{p:?}");
        assert!((p[2] as i32 - 184).abs() <= 2, "{p:?}");
        assert_eq!(p[3], 255);
        assert_eq!(*out.get_pixel(50, 20), WHITE);
    }

    #[test]
    fn flatten_text_draws_pixels() {
        let mut ann = Annotations::default();
        ann.push(Shape::Text {
            at: (5.0, 5.0),
            text: "Hi".into(),
            color: PALETTE[6],
            size: 20.0,
        });
        let out = flatten(&white(60, 40), &ann, &font());
        assert!(out.pixels().any(|p| p[0] < 100));
    }
}
