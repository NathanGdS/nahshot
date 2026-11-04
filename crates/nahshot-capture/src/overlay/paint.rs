//! Software painting of one monitor's slice of the overlay.
//!
//! Frames are softbuffer pixels (`0x00RRGGBB`); anything anti-aliased is drawn by
//! lifting a region into a tiny-skia pixmap and writing it back.

use super::toolbar::{self, ToolbarItem};
use crate::annotate::{Annotations, Tool};
use crate::font::Font;
use crate::geom::PxRect;
use crate::render::{draw_shapes, draw_text, Desktop};
use tiny_skia::{Paint, PixmapMut, Rect, Transform};

/// Darkening applied outside the selection (black veil at alpha 0.45).
const VEIL_KEEP: u32 = 140; // round(255 * (1 - 0.45))
/// Selection border color.
const BORDER: u32 = 0x00_1E_88_E5;
/// Size label font size and padding, in logical pixels.
const LABEL_SIZE: f32 = 12.0;
const LABEL_PAD: f32 = 4.0;
/// Side of the square resize handles, in logical pixels.
const HANDLE: f32 = 5.0;

/// Everything shared between monitors that a frame shows.
pub(crate) struct Scene<'a> {
    pub desktop: &'a Desktop,
    pub selection: Option<PxRect>,
    pub annotations: &'a Annotations,
    pub toolbar: &'a [(ToolbarItem, PxRect)],
    pub tool: Option<Tool>,
    pub color: usize,
    pub font: Option<&'a Font>,
    /// DPI scale of the monitor hosting the selection's chrome (label, handles, toolbar).
    pub scale: f32,
    /// The size label, already laid out.
    pub label: Option<(String, PxRect)>,
    /// Text caret while a text annotation is being typed.
    pub caret: Option<PxRect>,
}

/// One window's pixel buffer, positioned in virtual-desktop coordinates.
pub(crate) struct Frame<'a> {
    pub px: &'a mut [u32],
    pub area: PxRect,
}

/// The monitor's part of the desktop with the veil applied, as frame pixels.
pub(crate) fn veiled(desktop: &Desktop, area: PxRect) -> Vec<u32> {
    let mut out = vec![0; area.w as usize * area.h as usize];
    for_each_row(desktop, area, |row, src| {
        let dst = &mut out[row * area.w as usize..][..src.len() / 4];
        for (d, s) in dst.iter_mut().zip(src.as_chunks::<4>().0) {
            let k = |c: u8| (c as u32 * VEIL_KEEP + 127) / 255;
            *d = (k(s[0]) << 16) | (k(s[1]) << 8) | k(s[2]);
        }
    });
    out
}

/// Calls `f(row_in_area, rgba_bytes)` for each desktop row inside `area` (clipped).
fn for_each_row(desktop: &Desktop, area: PxRect, mut f: impl FnMut(usize, &[u8])) {
    let bounds = PxRect {
        x: desktop.origin.x,
        y: desktop.origin.y,
        w: desktop.image.width(),
        h: desktop.image.height(),
    };
    let Some(i) = area.intersect(&bounds) else {
        return;
    };
    let stride = desktop.image.width() as usize * 4;
    let raw = desktop.image.as_raw();
    for y in i.y..i.bottom() {
        let start = (y - bounds.y) as usize * stride + (i.x - bounds.x) as usize * 4;
        let row = (y - area.y) as usize;
        // Rows always start at the area's left edge: monitors lie inside the desktop.
        f(row, &raw[start..start + i.w as usize * 4]);
    }
}

impl Scene<'_> {
    /// Paints the scene over a frame that already holds the veiled background.
    pub fn paint(&self, frame: &mut Frame) {
        let Some(sel) = self.selection else { return };
        frame.unveil(self.desktop, sel);
        frame.with_pixmap(sel, |pixmap, (dx, dy)| {
            let corner = ((sel.x as f32) + dx, (sel.y as f32) + dy);
            draw_shapes(pixmap, corner, self.annotations, self.font);
        });
        if let Some(c) = self.caret {
            frame.fill(c, 0x00_FF_FF_FF);
        }
        frame.outline(sel, BORDER);
        self.paint_handles(frame, sel);
        if let (Some((text, rect)), Some(font)) = (&self.label, self.font) {
            let size = LABEL_SIZE * self.scale;
            let pad = LABEL_PAD * self.scale;
            frame.with_pixmap(*rect, |pixmap, (dx, dy)| {
                let r = to_skia(*rect, dx, dy);
                pixmap.fill_rect(r, &solid(0, 0, 0, 170), Transform::identity(), None);
                let at = (r.x() + pad, r.y() + pad / 2.0);
                draw_text(
                    pixmap,
                    at,
                    text,
                    crate::annotate::Color(255, 255, 255),
                    size,
                    font,
                );
            });
        }
        if let Some(bar) = toolbar::bounds(self.toolbar) {
            frame.with_pixmap(bar, |pixmap, offset| {
                toolbar::draw(
                    pixmap,
                    offset,
                    self.toolbar,
                    self.tool,
                    self.color,
                    self.font,
                );
            });
        }
    }

    fn paint_handles(&self, frame: &mut Frame, sel: PxRect) {
        let side = (HANDLE * self.scale).round().max(3.0) as i32;
        let (l, t, r, b) = (sel.x - 1, sel.y - 1, sel.right(), sel.bottom());
        let (cx, cy) = ((l + r) / 2, (t + b) / 2);
        for (x, y) in [
            (l, t),
            (cx, t),
            (r, t),
            (l, cy),
            (r, cy),
            (l, b),
            (cx, b),
            (r, b),
        ] {
            let square = PxRect {
                x: x - side / 2,
                y: y - side / 2,
                w: side as u32,
                h: side as u32,
            };
            frame.fill(square, BORDER);
        }
    }
}

/// Lays out the `"{w} × {h}"` label above the selection, or inside it at the top edge
/// of `screen`.
pub(crate) fn label_layout(
    sel: PxRect,
    screen: PxRect,
    scale: f32,
    font: &Font,
) -> (String, PxRect) {
    let text = format!("{} \u{00D7} {}", sel.w, sel.h);
    let size = LABEL_SIZE * scale;
    let pad = LABEL_PAD * scale;
    let w = (font.text_width(&text, size) + 2.0 * pad).ceil() as u32;
    let h = (size * 1.4 + pad).ceil() as u32;
    let gap = pad.round() as i32;
    let above = sel.y - gap - h as i32;
    let y = if above >= screen.y {
        above
    } else {
        sel.y + gap
    };
    let x = sel.x.max(screen.x).min(screen.right() - w as i32);
    (text, PxRect { x, y, w, h })
}

fn solid(r: u8, g: u8, b: u8, a: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p
}

fn to_skia(r: PxRect, dx: f32, dy: f32) -> Rect {
    Rect::from_xywh(r.x as f32 + dx, r.y as f32 + dy, r.w as f32, r.h as f32)
        .expect("layout rects are non-empty")
}

impl Frame<'_> {
    /// Local `(x, y, w, h)` of `r` clipped to the frame, if visible.
    fn clip(&self, r: PxRect) -> Option<PxRect> {
        r.intersect(&self.area).map(|i| PxRect {
            x: i.x - self.area.x,
            y: i.y - self.area.y,
            ..i
        })
    }

    fn rows(&mut self, local: PxRect) -> impl Iterator<Item = &mut [u32]> {
        let w = self.area.w as usize;
        self.px
            .chunks_exact_mut(w)
            .skip(local.y as usize)
            .take(local.h as usize)
            .map(move |row| &mut row[local.x as usize..][..local.w as usize])
    }

    /// Fills `r` (desktop coordinates) with an opaque color.
    pub fn fill(&mut self, r: PxRect, color: u32) {
        if let Some(local) = self.clip(r) {
            self.rows(local).for_each(|row| row.fill(color));
        }
    }

    /// Draws a 1 px border just outside `r`.
    fn outline(&mut self, r: PxRect, color: u32) {
        let (l, t) = (r.x - 1, r.y - 1);
        let (w, h) = (r.w + 2, r.h + 2);
        for edge in [
            PxRect {
                x: l,
                y: t,
                w,
                h: 1,
            },
            PxRect {
                x: l,
                y: r.bottom(),
                w,
                h: 1,
            },
            PxRect {
                x: l,
                y: t,
                w: 1,
                h,
            },
            PxRect {
                x: r.right(),
                y: t,
                w: 1,
                h,
            },
        ] {
            self.fill(edge, color);
        }
    }

    /// Restores the un-veiled screenshot inside `r`.
    fn unveil(&mut self, desktop: &Desktop, r: PxRect) {
        let Some(i) = r.intersect(&self.area) else {
            return;
        };
        let (ax, ay, w) = (self.area.x, self.area.y, self.area.w as usize);
        let px = &mut *self.px;
        for_each_row(desktop, i, |row, src| {
            let y = (i.y - ay) as usize + row;
            let dst = &mut px[y * w + (i.x - ax) as usize..][..src.len() / 4];
            for (d, s) in dst.iter_mut().zip(src.as_chunks::<4>().0) {
                *d = ((s[0] as u32) << 16) | ((s[1] as u32) << 8) | s[2] as u32;
            }
        });
    }

    /// Lifts `r` into an opaque tiny-skia pixmap, lets `draw` paint on it, and writes it
    /// back. `draw` receives the translation from desktop to pixmap coordinates.
    fn with_pixmap(&mut self, r: PxRect, draw: impl FnOnce(&mut PixmapMut, (f32, f32))) {
        let Some(local) = self.clip(r) else { return };
        let mut bytes = Vec::with_capacity(local.w as usize * local.h as usize * 4);
        for row in self.rows(local) {
            for &p in row.iter() {
                bytes.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
            }
        }
        if let Some(mut pixmap) = PixmapMut::from_bytes(&mut bytes, local.w, local.h) {
            let origin = (local.x + self.area.x, local.y + self.area.y);
            draw(&mut pixmap, (-origin.0 as f32, -origin.1 as f32));
        }
        let mut src = bytes.as_chunks::<4>().0.iter();
        for row in self.rows(local) {
            for (d, s) in row.iter_mut().zip(&mut src) {
                *d = ((s[0] as u32) << 16) | ((s[1] as u32) << 8) | s[2] as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PxPoint;
    use image::{Rgba, RgbaImage};

    fn desktop() -> Desktop {
        Desktop {
            origin: PxPoint { x: -4, y: 0 },
            image: RgbaImage::from_pixel(8, 4, Rgba([200, 100, 0, 255])),
        }
    }

    #[test]
    fn veil_darkens_to_55_percent() {
        let area = PxRect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let px = veiled(&desktop(), area);
        assert_eq!(px.len(), 16);
        assert_eq!(px[0], (110 << 16) | (55 << 8));
    }

    #[test]
    fn unveil_restores_selection_only() {
        let d = desktop();
        let area = PxRect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let mut px = veiled(&d, area);
        let mut frame = Frame { px: &mut px, area };
        frame.unveil(
            &d,
            PxRect {
                x: -2,
                y: 1,
                w: 3,
                h: 1,
            },
        );
        assert_eq!(px[4], (200 << 16) | (100 << 8));
        assert_eq!(px[5], (110 << 16) | (55 << 8));
        assert_eq!(px[0], (110 << 16) | (55 << 8));
    }
}
