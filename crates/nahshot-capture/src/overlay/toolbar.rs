//! Toolbar glued to the selection: layout, hit-testing and icon drawing.

use crate::annotate::{Color, Tool, PALETTE};
use crate::font::Font;
use crate::geom::{PxPoint, PxRect};
use crate::render::draw_text;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

/// One toolbar button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItem {
    Tool(Tool),
    /// Index into [`crate::annotate::PALETTE`].
    Color(usize),
    Undo,
    Copy,
    SaveAs,
    Close,
}

/// Names of the [`PALETTE`] colors, in the same order.
const COLOR_NAMES: [&str; 8] = [
    "Vermelho", "Laranja", "Amarelo", "Verde", "Azul", "Roxo", "Preto", "Branco",
];

impl ToolbarItem {
    /// What the button does, shown while the pointer rests on it.
    pub fn tooltip(self) -> String {
        let text = match self {
            ToolbarItem::Tool(Tool::Pen) => "Caneta",
            ToolbarItem::Tool(Tool::Line) => "Linha",
            ToolbarItem::Tool(Tool::Arrow) => "Seta",
            ToolbarItem::Tool(Tool::Rect) => "Retângulo",
            ToolbarItem::Tool(Tool::Ellipse) => "Elipse",
            ToolbarItem::Tool(Tool::Marker) => "Marca-texto",
            ToolbarItem::Tool(Tool::Text) => "Texto",
            ToolbarItem::Color(i) => return format!("Cor: {}", COLOR_NAMES[i]),
            ToolbarItem::Undo => "Desfazer (Ctrl+Z)",
            ToolbarItem::Copy => "Copiar (Ctrl+C)",
            ToolbarItem::SaveAs => "Salvar como (Ctrl+S)",
            ToolbarItem::Close => "Fechar (Esc)",
        };
        text.to_string()
    }
}

/// Button side in logical pixels.
const BUTTON: f32 = 32.0;
/// Spacing between buttons (and between the bar and the selection), in logical pixels.
const GAP: f32 = 4.0;

/// Buttons in display order.
const ITEMS: [ToolbarItem; 19] = [
    ToolbarItem::Tool(Tool::Pen),
    ToolbarItem::Tool(Tool::Line),
    ToolbarItem::Tool(Tool::Arrow),
    ToolbarItem::Tool(Tool::Rect),
    ToolbarItem::Tool(Tool::Ellipse),
    ToolbarItem::Tool(Tool::Marker),
    ToolbarItem::Tool(Tool::Text),
    ToolbarItem::Color(0),
    ToolbarItem::Color(1),
    ToolbarItem::Color(2),
    ToolbarItem::Color(3),
    ToolbarItem::Color(4),
    ToolbarItem::Color(5),
    ToolbarItem::Color(6),
    ToolbarItem::Color(7),
    ToolbarItem::Undo,
    ToolbarItem::Copy,
    ToolbarItem::SaveAs,
    ToolbarItem::Close,
];

/// Lays the buttons out in a row below the selection, above it if there is no room,
/// or inside its bottom corner if neither fits on `screen`.
pub fn layout(sel: PxRect, screen: PxRect, scale: f32) -> Vec<(ToolbarItem, PxRect)> {
    let button = (BUTTON * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    let n = ITEMS.len() as i32;
    let width = n * button + (n - 1) * gap;
    let below = sel.bottom() + gap;
    let above = sel.y - gap - button;
    let (x, y) = if below + button <= screen.bottom() {
        (sel.right() - width, below)
    } else if above >= screen.y {
        (sel.right() - width, above)
    } else {
        (sel.right() - gap - width, sel.bottom() - gap - button)
    };
    let x = x.min(screen.right() - width).max(screen.x);
    ITEMS
        .iter()
        .zip(0..)
        .map(|(&item, i)| {
            let r = PxRect {
                x: x + i * (button + gap),
                y,
                w: button as u32,
                h: button as u32,
            };
            (item, r)
        })
        .collect()
}

/// The button under `p`, if any.
pub fn hit(items: &[(ToolbarItem, PxRect)], p: PxPoint) -> Option<ToolbarItem> {
    items.iter().find(|(_, r)| r.contains(p)).map(|(i, _)| *i)
}

/// The bar's background: the buttons' bounding box plus a gap-wide margin.
pub(crate) fn bounds(items: &[(ToolbarItem, PxRect)]) -> Option<PxRect> {
    let (first, last) = (items.first()?.1, items.last()?.1);
    let pad = (first.w as f32 * GAP / BUTTON).round() as i32;
    Some(PxRect {
        x: first.x - pad,
        y: first.y - pad,
        w: (last.right() - first.x + 2 * pad) as u32,
        h: first.h + 2 * pad as u32,
    })
}

/// Draws the bar; `offset` maps desktop coordinates to `pixmap` coordinates.
pub(crate) fn draw(
    pixmap: &mut PixmapMut,
    offset: (f32, f32),
    items: &[(ToolbarItem, PxRect)],
    tool: Option<Tool>,
    color: usize,
    font: Option<&Font>,
) {
    let Some(bar) = bounds(items) else { return };
    let tf = Transform::from_translate(offset.0, offset.1);
    pixmap.fill_rect(skia_rect(bar), &paint(BAR, 245), tf, None);
    for &(item, r) in items {
        let active = match item {
            ToolbarItem::Tool(t) => tool == Some(t),
            _ => false,
        };
        if active {
            pixmap.fill_rect(skia_rect(r), &paint(ACTIVE, 255), tf, None);
        }
        let icon = Icon::new(r);
        match item {
            ToolbarItem::Tool(t) => icon.tool(pixmap, tf, t, font),
            ToolbarItem::Color(i) => icon.swatch(pixmap, tf, PALETTE[i], i == color),
            ToolbarItem::Undo => icon.undo(pixmap, tf),
            ToolbarItem::Copy => icon.copy(pixmap, tf),
            ToolbarItem::SaveAs => icon.save(pixmap, tf),
            ToolbarItem::Close => icon.close(pixmap, tf),
        }
    }
}

const BAR: Color = Color(0x26, 0x26, 0x26);
const ACTIVE: Color = Color(0x1E, 0x88, 0xE5);
const INK: Color = Color(0xEE, 0xEE, 0xEE);
const HIGHLIGHT: Color = Color(0xFD, 0xD8, 0x35);

fn paint(c: Color, alpha: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, alpha);
    p.anti_alias = true;
    p
}

fn skia_rect(r: PxRect) -> Rect {
    Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32).expect("non-empty button")
}

/// Icon geometry for one button: a unit square mapped onto the button's inner area.
struct Icon {
    x: f32,
    y: f32,
    side: f32,
}

impl Icon {
    fn new(r: PxRect) -> Self {
        let inset = r.w as f32 * 0.25;
        Icon {
            x: r.x as f32 + inset,
            y: r.y as f32 + inset,
            side: r.w as f32 - 2.0 * inset,
        }
    }

    /// Maps unit coordinates (0..1) to desktop coordinates.
    fn at(&self, u: f32, v: f32) -> (f32, f32) {
        (self.x + u * self.side, self.y + v * self.side)
    }

    fn line_width(&self) -> f32 {
        (self.side / 9.0).max(1.0)
    }

    fn stroke(&self, pixmap: &mut PixmapMut, tf: Transform, path: Option<Path>, c: Color, a: u8) {
        let stroke = Stroke {
            width: self.line_width(),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        if let Some(path) = path {
            pixmap.stroke_path(&path, &paint(c, a), &stroke, tf, None);
        }
    }

    fn fill(&self, pixmap: &mut PixmapMut, tf: Transform, path: Option<Path>, c: Color) {
        if let Some(path) = path {
            pixmap.fill_path(&path, &paint(c, 255), FillRule::Winding, tf, None);
        }
    }

    fn polyline(&self, pts: &[(f32, f32)]) -> Option<Path> {
        let mut pb = PathBuilder::new();
        let (first, rest) = pts.split_first()?;
        let p = self.at(first.0, first.1);
        pb.move_to(p.0, p.1);
        for &(u, v) in rest {
            let p = self.at(u, v);
            pb.line_to(p.0, p.1);
        }
        pb.finish()
    }

    /// Filled triangular head at `tip`, pointing along `dir` (unit coordinates).
    fn head(&self, pixmap: &mut PixmapMut, tf: Transform, tip: (f32, f32), dir: (f32, f32)) {
        let len = dir.0.hypot(dir.1);
        let (ux, uy) = (dir.0 / len, dir.1 / len);
        let (back, half) = (0.32, 0.2);
        let base = (tip.0 - ux * back, tip.1 - uy * back);
        let [a, b, c] = [
            tip,
            (base.0 - uy * half, base.1 + ux * half),
            (base.0 + uy * half, base.1 - ux * half),
        ]
        .map(|(u, v)| self.at(u, v));
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        pb.line_to(c.0, c.1);
        pb.close();
        self.fill(pixmap, tf, pb.finish(), INK);
    }

    fn unit_rect(&self, l: f32, t: f32, r: f32, b: f32) -> Option<Rect> {
        let (a, z) = (self.at(l, t), self.at(r, b));
        Rect::from_ltrb(a.0, a.1, z.0, z.1)
    }

    fn tool(&self, pixmap: &mut PixmapMut, tf: Transform, tool: Tool, font: Option<&Font>) {
        match tool {
            Tool::Pen => {
                let mut pb = PathBuilder::new();
                let (a, c1, b, c2, z) = (
                    self.at(0.0, 0.8),
                    self.at(0.25, 0.0),
                    self.at(0.5, 0.5),
                    self.at(0.75, 1.0),
                    self.at(1.0, 0.2),
                );
                pb.move_to(a.0, a.1);
                pb.quad_to(c1.0, c1.1, b.0, b.1);
                pb.quad_to(c2.0, c2.1, z.0, z.1);
                self.stroke(pixmap, tf, pb.finish(), INK, 255);
            }
            Tool::Line => self.stroke(
                pixmap,
                tf,
                self.polyline(&[(0.0, 1.0), (1.0, 0.0)]),
                INK,
                255,
            ),
            Tool::Arrow => {
                self.stroke(
                    pixmap,
                    tf,
                    self.polyline(&[(0.0, 1.0), (0.8, 0.2)]),
                    INK,
                    255,
                );
                self.head(pixmap, tf, (1.0, 0.0), (1.0, -1.0));
            }
            Tool::Rect => {
                let path = self
                    .unit_rect(0.0, 0.15, 1.0, 0.85)
                    .map(PathBuilder::from_rect);
                self.stroke(pixmap, tf, path, INK, 255);
            }
            Tool::Ellipse => {
                let path = self
                    .unit_rect(0.0, 0.1, 1.0, 0.9)
                    .and_then(PathBuilder::from_oval);
                self.stroke(pixmap, tf, path, INK, 255);
            }
            Tool::Marker => {
                let stroke = Stroke {
                    width: self.side * 0.35,
                    line_cap: LineCap::Butt,
                    ..Stroke::default()
                };
                if let Some(path) = self.polyline(&[(0.0, 0.5), (1.0, 0.5)]) {
                    pixmap.stroke_path(&path, &paint(HIGHLIGHT, 160), &stroke, tf, None);
                }
            }
            Tool::Text => {
                if let Some(font) = font {
                    let size = self.side * 1.3;
                    let w = font.text_width("T", size);
                    let (x, y) = self.at(0.5, 0.5);
                    let (tx, ty) = (tf.tx, tf.ty);
                    let at = (x + tx - w / 2.0, y + ty - size * 0.62);
                    draw_text(pixmap, at, "T", INK, size, font);
                }
            }
        }
    }

    fn swatch(&self, pixmap: &mut PixmapMut, tf: Transform, c: Color, selected: bool) {
        let (cx, cy) = self.at(0.5, 0.5);
        if selected {
            let ring = PathBuilder::from_circle(cx, cy, self.side * 0.62);
            self.stroke(pixmap, tf, ring, INK, 255);
        }
        self.fill(
            pixmap,
            tf,
            PathBuilder::from_circle(cx, cy, self.side * 0.42),
            c,
        );
        // A thin outline keeps black and white swatches visible on the dark bar.
        let edge = PathBuilder::from_circle(cx, cy, self.side * 0.42);
        self.stroke(pixmap, tf, edge, Color(0x80, 0x80, 0x80), 255);
    }

    /// Counter-clockwise half-turn arrow (↶).
    fn undo(&self, pixmap: &mut PixmapMut, tf: Transform) {
        let mut pb = PathBuilder::new();
        let (a, c1, c2, z) = (
            self.at(0.9, 0.9),
            self.at(0.9, 0.05),
            self.at(0.15, 0.05),
            self.at(0.15, 0.6),
        );
        pb.move_to(a.0, a.1);
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, z.0, z.1);
        self.stroke(pixmap, tf, pb.finish(), INK, 255);
        self.head(pixmap, tf, (0.15, 0.95), (0.0, 1.0));
    }

    fn copy(&self, pixmap: &mut PixmapMut, tf: Transform) {
        let back = self
            .unit_rect(0.0, 0.0, 0.65, 0.65)
            .map(PathBuilder::from_rect);
        self.stroke(pixmap, tf, back, INK, 255);
        let front = self.unit_rect(0.35, 0.35, 1.0, 1.0);
        if let Some(r) = front {
            pixmap.fill_rect(r, &paint(BAR, 255), tf, None);
        }
        self.stroke(pixmap, tf, front.map(PathBuilder::from_rect), INK, 255);
    }

    /// Down arrow into a tray.
    fn save(&self, pixmap: &mut PixmapMut, tf: Transform) {
        self.stroke(
            pixmap,
            tf,
            self.polyline(&[(0.5, 0.0), (0.5, 0.45)]),
            INK,
            255,
        );
        self.head(pixmap, tf, (0.5, 0.75), (0.0, 1.0));
        let tray = [(0.0, 0.7), (0.0, 1.0), (1.0, 1.0), (1.0, 0.7)];
        self.stroke(pixmap, tf, self.polyline(&tray), INK, 255);
    }

    fn close(&self, pixmap: &mut PixmapMut, tf: Transform) {
        self.stroke(
            pixmap,
            tf,
            self.polyline(&[(0.1, 0.1), (0.9, 0.9)]),
            INK,
            255,
        );
        self.stroke(
            pixmap,
            tf,
            self.polyline(&[(0.9, 0.1), (0.1, 0.9)]),
            INK,
            255,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: PxRect = PxRect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PxRect {
        PxRect { x, y, w, h }
    }

    fn inside(outer: PxRect, inner: PxRect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }

    #[test]
    fn toolbar_below_selection() {
        let sel = rect(400, 300, 800, 400);
        let items = layout(sel, SCREEN, 1.0);
        assert!(!items.is_empty());
        assert!(items.iter().all(|(_, r)| r.y > sel.y + sel.h as i32));
        assert!(items.iter().all(|(_, r)| inside(SCREEN, *r)));
    }

    #[test]
    fn toolbar_flips_above_at_bottom_edge() {
        let sel = rect(400, 600, 800, 470);
        let items = layout(sel, SCREEN, 1.0);
        assert!(!items.is_empty());
        assert!(items.iter().all(|(_, r)| r.bottom() < sel.y));
        assert!(items.iter().all(|(_, r)| inside(SCREEN, *r)));
    }

    #[test]
    fn toolbar_goes_inside_when_selection_fills_screen() {
        let items = layout(SCREEN, SCREEN, 1.0);
        assert!(!items.is_empty());
        assert!(items.iter().all(|(_, r)| inside(SCREEN, *r)));
        assert!(items.iter().all(|(_, r)| r.y > SCREEN.h as i32 / 2));
    }

    #[test]
    fn toolbar_stays_on_screen_near_left_edge() {
        let items = layout(rect(0, 100, 50, 50), SCREEN, 1.0);
        assert!(!items.is_empty());
        assert!(items.iter().all(|(_, r)| inside(SCREEN, *r)));
    }

    #[test]
    fn hit_returns_item() {
        let items = layout(rect(400, 300, 800, 400), SCREEN, 1.0);
        let first = items[0].1;
        let center = PxPoint {
            x: first.x + first.w as i32 / 2,
            y: first.y + first.h as i32 / 2,
        };
        assert_eq!(hit(&items, center), Some(ToolbarItem::Tool(Tool::Pen)));
        assert_eq!(hit(&items, PxPoint { x: 0, y: 0 }), None);
    }

    #[test]
    fn every_button_has_a_tooltip() {
        assert_eq!(COLOR_NAMES.len(), PALETTE.len());
        for item in ITEMS {
            assert!(!item.tooltip().is_empty(), "{item:?}");
        }
        assert_eq!(ToolbarItem::Color(0).tooltip(), "Cor: Vermelho");
        assert_eq!(ToolbarItem::Undo.tooltip(), "Desfazer (Ctrl+Z)");
    }

    #[test]
    fn layout_scales_with_dpi() {
        let items = layout(rect(100, 100, 1200, 300), rect(0, 0, 2880, 1620), 1.5);
        assert!(!items.is_empty());
        assert!(items.iter().all(|(_, r)| r.w == 48 && r.h == 48));
    }
}
