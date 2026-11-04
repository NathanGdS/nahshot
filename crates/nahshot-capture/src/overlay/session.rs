//! State of one open capture, shared by every monitor window, and its input handling.
//!
//! Everything here is in virtual-desktop physical pixels and free of winit windows, so
//! the interaction logic is unit-testable.

use super::paint::{label_layout, Scene};
use super::toolbar::{self, ToolbarItem};
use super::{Action, CaptureResult};
use crate::annotate::{Annotations, Pt, Shape, Tool, PALETTE, STROKE_WIDTH, TEXT_SIZE};
use crate::font::Font;
use crate::geom::{Handle, PxPoint, PxRect, Selection};
use crate::render::{crop, flatten, Desktop};
use chrono::NaiveDateTime;
use std::rc::Rc;
use winit::window::CursorIcon;

/// Pointer slack around selection edges, in logical pixels.
const EDGE_TOLERANCE: f32 = 6.0;
/// Extra margin painted around the selection (border and handles), in logical pixels.
const CHROME_MARGIN: f32 = 8.0;

/// A captured monitor: bounds and DPI scale.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Monitor {
    pub area: PxRect,
    pub scale: f32,
}

/// A keyboard command, already decoded from the platform event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Key {
    Escape,
    Enter,
    Backspace,
    Undo,
    Copy,
    SaveAs,
    Text(String),
}

/// What the overlay must do after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Nothing,
    Redraw,
    Finish(Action),
}

#[derive(Debug)]
enum Gesture {
    Idle,
    Selecting,
    /// Resizing/moving: every move is re-applied to the selection as it was at press.
    Adjusting {
        handle: Handle,
        from: PxPoint,
        start: Selection,
    },
    Drawing,
}

/// One open capture.
pub(crate) struct Session {
    desktop: Desktop,
    monitors: Vec<Monitor>,
    font: Option<Rc<Font>>,
    captured_at: NaiveDateTime,
    sel: Selection,
    ann: Annotations,
    tool: Option<Tool>,
    color: usize,
    gesture: Gesture,
    cursor: Option<PxPoint>,
    /// The topmost annotation is a text box still being typed.
    editing: bool,
    toolbar: Vec<(ToolbarItem, PxRect)>,
    label: Option<(String, PxRect)>,
    painted: Option<PxRect>,
}

impl Session {
    /// Starts a capture over `desktop`; `monitors` must not be empty.
    pub fn new(
        desktop: Desktop,
        monitors: Vec<Monitor>,
        font: Option<Rc<Font>>,
        captured_at: NaiveDateTime,
    ) -> Self {
        Session {
            desktop,
            monitors,
            font,
            captured_at,
            sel: Selection::default(),
            ann: Annotations::default(),
            tool: None,
            color: 0,
            gesture: Gesture::Idle,
            cursor: None,
            editing: false,
            toolbar: Vec::new(),
            label: None,
            painted: None,
        }
    }

    /// The stitched screenshot.
    pub fn desktop(&self) -> &Desktop {
        &self.desktop
    }

    /// Whether a text box is being typed (the IME should be enabled).
    pub fn editing(&self) -> bool {
        self.editing
    }

    /// What every window paints.
    pub fn scene(&self) -> Scene<'_> {
        Scene {
            desktop: &self.desktop,
            selection: self.sel.rect(),
            annotations: &self.ann,
            toolbar: &self.toolbar,
            tool: self.tool,
            color: self.color,
            font: self.font.as_deref(),
            scale: self.chrome_scale(),
            label: self.label.clone(),
            caret: self.caret(),
        }
    }

    /// The area whose pixels changed since the last call (old and new chrome).
    pub fn take_damage(&mut self) -> Option<PxRect> {
        let now = self.chrome_bounds();
        let damage = union(self.painted, now);
        self.painted = now;
        damage
    }

    /// Pointer moved to `p`.
    pub fn moved(&mut self, p: PxPoint) -> Outcome {
        self.cursor = Some(p);
        match self.gesture {
            Gesture::Idle => return Outcome::Nothing,
            Gesture::Selecting => self.sel.update(p),
            Gesture::Adjusting {
                handle,
                from,
                ref start,
            } => {
                let mut s = start.clone();
                s.drag(handle, from, p);
                self.sel = s;
            }
            Gesture::Drawing => self.extend_shape(p),
        }
        self.relayout();
        Outcome::Redraw
    }

    /// Left button pressed at the last pointer position.
    pub fn pressed(&mut self) -> Outcome {
        let Some(p) = self.cursor else {
            return Outcome::Nothing;
        };
        if self.editing {
            self.commit_text();
        }
        if let Some(item) = toolbar::hit(&self.toolbar, p) {
            return self.click(item);
        }
        let rect = self.sel.rect();
        if let (Some(tool), Some(r)) = (self.tool, rect) {
            if r.contains(p) {
                self.start_shape(tool, r, p);
                return Outcome::Redraw;
            }
        }
        let tolerance = (EDGE_TOLERANCE * self.monitor_at(p).scale).round() as i32;
        if let Some(handle) = self.sel.hit(p, tolerance) {
            self.gesture = Gesture::Adjusting {
                handle,
                from: p,
                start: self.sel.clone(),
            };
        } else if self.ann.is_empty() {
            self.sel.begin(p);
            self.gesture = Gesture::Selecting;
        } else {
            // Starting over would silently drop the drawings; undo them first.
            return Outcome::Nothing;
        }
        self.relayout();
        Outcome::Redraw
    }

    /// Left button released.
    pub fn released(&mut self) -> Outcome {
        let was_drawing = matches!(self.gesture, Gesture::Drawing);
        self.gesture = Gesture::Idle;
        if was_drawing && self.ann.shapes().last().is_some_and(is_degenerate) {
            self.ann.undo();
        }
        Outcome::Redraw
    }

    /// A keyboard command.
    pub fn key(&mut self, key: Key) -> Outcome {
        if self.editing {
            return self.edit_text(key);
        }
        match key {
            Key::Escape => Outcome::Finish(Action::Close),
            Key::Copy if self.sel.rect().is_some() => Outcome::Finish(Action::Copy),
            Key::SaveAs if self.sel.rect().is_some() => Outcome::Finish(Action::SaveAs),
            Key::Undo => {
                self.ann.undo();
                Outcome::Redraw
            }
            _ => Outcome::Nothing,
        }
    }

    /// Cursor shape for the current pointer position.
    pub fn cursor_icon(&self) -> CursorIcon {
        let Some(p) = self.cursor else {
            return CursorIcon::Default;
        };
        match self.gesture {
            Gesture::Adjusting { handle, .. } => return handle_icon(handle),
            Gesture::Selecting | Gesture::Drawing => return CursorIcon::Crosshair,
            Gesture::Idle => {}
        }
        if toolbar::hit(&self.toolbar, p).is_some() {
            return CursorIcon::Pointer;
        }
        let inside = self.sel.rect().is_some_and(|r| r.contains(p));
        match self.tool {
            Some(Tool::Text) if inside => return CursorIcon::Text,
            Some(_) if inside => return CursorIcon::Crosshair,
            _ => {}
        }
        let tolerance = (EDGE_TOLERANCE * self.monitor_at(p).scale).round() as i32;
        match self.sel.hit(p, tolerance) {
            Some(h) => handle_icon(h),
            None if self.ann.is_empty() => CursorIcon::Crosshair,
            None => CursorIcon::Default,
        }
    }

    /// Ends the capture: the selection, flattened with its annotations, if any.
    pub fn finish(mut self, action: Action) -> CaptureResult {
        if self.editing {
            self.commit_text();
        }
        let image = self
            .sel
            .rect()
            .map(|r| flatten(&crop(&self.desktop, r), &self.ann, self.font.as_deref()));
        CaptureResult {
            action,
            image,
            captured_at: self.captured_at,
        }
    }

    fn click(&mut self, item: ToolbarItem) -> Outcome {
        match item {
            ToolbarItem::Tool(Tool::Text) if self.font.is_none() => return Outcome::Nothing,
            ToolbarItem::Tool(t) => self.tool = (self.tool != Some(t)).then_some(t),
            ToolbarItem::Color(i) => self.color = i,
            ToolbarItem::Undo => {
                self.ann.undo();
            }
            ToolbarItem::Copy => return Outcome::Finish(Action::Copy),
            ToolbarItem::SaveAs => return Outcome::Finish(Action::SaveAs),
            ToolbarItem::Close => return Outcome::Finish(Action::Close),
        }
        Outcome::Redraw
    }

    fn start_shape(&mut self, tool: Tool, sel: PxRect, p: PxPoint) {
        let scale = self.monitor_at(p).scale;
        let (color, width, at) = (PALETTE[self.color], STROKE_WIDTH * scale, relative(sel, p));
        let shape = match tool {
            Tool::Pen => Shape::Pen {
                pts: vec![at],
                color,
                width,
            },
            Tool::Line => Shape::Line {
                a: at,
                b: at,
                color,
                width,
            },
            Tool::Arrow => Shape::Arrow {
                a: at,
                b: at,
                color,
                width,
            },
            Tool::Rect => Shape::Rect {
                a: at,
                b: at,
                color,
                width,
            },
            Tool::Ellipse => Shape::Ellipse {
                a: at,
                b: at,
                color,
                width,
            },
            Tool::Marker => Shape::Marker {
                pts: vec![at],
                color,
            },
            Tool::Text => {
                let size = TEXT_SIZE * scale;
                self.ann.push(Shape::Text {
                    at: (at.0, (at.1 - size / 2.0).max(0.0)),
                    text: String::new(),
                    color,
                    size,
                });
                self.editing = true;
                return;
            }
        };
        self.ann.push(shape);
        self.gesture = Gesture::Drawing;
    }

    fn extend_shape(&mut self, p: PxPoint) {
        let Some(sel) = self.sel.rect() else { return };
        let at = relative(sel, p);
        match self.ann.last_mut() {
            Some(Shape::Pen { pts, .. } | Shape::Marker { pts, .. }) => {
                if pts.last() != Some(&at) {
                    pts.push(at);
                }
            }
            Some(
                Shape::Line { b, .. }
                | Shape::Arrow { b, .. }
                | Shape::Rect { b, .. }
                | Shape::Ellipse { b, .. },
            ) => *b = at,
            _ => {}
        }
    }

    fn edit_text(&mut self, key: Key) -> Outcome {
        let Some(Shape::Text { text, .. }) = self.ann.last_mut() else {
            self.editing = false;
            return Outcome::Nothing;
        };
        match key {
            Key::Text(s) => text.push_str(&s),
            Key::Backspace => {
                text.pop();
            }
            Key::Enter => self.commit_text(),
            Key::Escape | Key::Undo => {
                self.ann.undo();
                self.editing = false;
            }
            Key::Copy | Key::SaveAs => {
                self.commit_text();
                return self.key(key);
            }
        }
        Outcome::Redraw
    }

    /// Keeps the text box being typed, unless it is empty.
    fn commit_text(&mut self) {
        self.editing = false;
        if matches!(self.ann.shapes().last(), Some(Shape::Text { text, .. }) if text.trim().is_empty())
        {
            self.ann.undo();
        }
    }

    fn caret(&self) -> Option<PxRect> {
        let font = self.font.as_deref().filter(|_| self.editing)?;
        let sel = self.sel.rect()?;
        let Some(Shape::Text { at, text, size, .. }) = self.ann.shapes().last() else {
            return None;
        };
        Some(PxRect {
            x: sel.x + (at.0 + font.text_width(text, *size)).round() as i32,
            y: sel.y + at.1.round() as i32,
            w: (size / TEXT_SIZE).round().max(1.0) as u32,
            h: (size * 1.2).round() as u32,
        })
    }

    /// Recomputes the toolbar and size label after the selection changed.
    fn relayout(&mut self) {
        let Some(sel) = self.sel.rect() else {
            self.toolbar.clear();
            self.label = None;
            return;
        };
        let host = *self.monitor_at(corner(sel));
        self.toolbar = toolbar::layout(sel, host.area, host.scale);
        let top = *self.monitor_at(PxPoint { x: sel.x, y: sel.y });
        self.label = self
            .font
            .as_deref()
            .map(|f| label_layout(sel, top.area, top.scale, f));
    }

    fn chrome_scale(&self) -> f32 {
        self.sel
            .rect()
            .map_or(1.0, |r| self.monitor_at(corner(r)).scale)
    }

    /// Everything drawn over the veil: selection with border/handles, label, toolbar.
    fn chrome_bounds(&self) -> Option<PxRect> {
        let sel = self.sel.rect()?;
        let m = (CHROME_MARGIN * self.chrome_scale()).ceil() as i32;
        let grown = PxRect {
            x: sel.x - m,
            y: sel.y - m,
            w: sel.w + 2 * m as u32,
            h: sel.h + 2 * m as u32,
        };
        let bounds = union(Some(grown), self.label.as_ref().map(|l| l.1));
        union(bounds, toolbar::bounds(&self.toolbar))
    }

    /// The monitor containing `p`, or the nearest one.
    fn monitor_at(&self, p: PxPoint) -> &Monitor {
        let distance = |m: &&Monitor| {
            let a = m.area;
            let dx = (a.x - p.x).max(p.x - (a.right() - 1)).max(0) as i64;
            let dy = (a.y - p.y).max(p.y - (a.bottom() - 1)).max(0) as i64;
            dx * dx + dy * dy
        };
        self.monitors
            .iter()
            .min_by_key(distance)
            .expect("a session always has a monitor")
    }
}

/// The selection's bottom-right pixel, where the toolbar is anchored.
fn corner(r: PxRect) -> PxPoint {
    PxPoint {
        x: r.right() - 1,
        y: r.bottom() - 1,
    }
}

/// `p` relative to the selection corner, clamped inside the selection.
fn relative(sel: PxRect, p: PxPoint) -> Pt {
    (
        (p.x - sel.x).clamp(0, sel.w as i32) as f32,
        (p.y - sel.y).clamp(0, sel.h as i32) as f32,
    )
}

/// A two-point shape whose points coincide (a click without a drag).
fn is_degenerate(s: &Shape) -> bool {
    match s {
        Shape::Line { a, b, .. }
        | Shape::Arrow { a, b, .. }
        | Shape::Rect { a, b, .. }
        | Shape::Ellipse { a, b, .. } => a == b,
        _ => false,
    }
}

fn handle_icon(h: Handle) -> CursorIcon {
    match h {
        Handle::Move => CursorIcon::Move,
        Handle::N | Handle::S => CursorIcon::NsResize,
        Handle::E | Handle::W => CursorIcon::EwResize,
        Handle::NW | Handle::SE => CursorIcon::NwseResize,
        Handle::NE | Handle::SW => CursorIcon::NeswResize,
    }
}

fn union(a: Option<PxRect>, b: Option<PxRect>) -> Option<PxRect> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (l, t) = (a.x.min(b.x), a.y.min(b.y));
            let (r, bt) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
            Some(PxRect {
                x: l,
                y: t,
                w: (r - l) as u32,
                h: (bt - t) as u32,
            })
        }
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn session(font: bool) -> Session {
        let shot = |x: i32| Monitor {
            area: PxRect {
                x,
                y: 0,
                w: 200,
                h: 100,
            },
            scale: 1.0,
        };
        let desktop = Desktop {
            origin: PxPoint { x: -200, y: 0 },
            image: RgbaImage::from_pixel(400, 100, Rgba([255, 255, 255, 255])),
        };
        let font = font.then(|| Rc::new(Font::system().expect("system font")));
        Session::new(
            desktop,
            vec![shot(-200), shot(0)],
            font,
            NaiveDateTime::default(),
        )
    }

    fn drag(s: &mut Session, from: (i32, i32), to: (i32, i32)) {
        s.moved(PxPoint {
            x: from.0,
            y: from.1,
        });
        s.pressed();
        s.moved(PxPoint { x: to.0, y: to.1 });
        s.released();
    }

    #[test]
    fn selection_spans_monitors() {
        let mut s = session(false);
        drag(&mut s, (-50, 10), (50, 60));
        let r = s.finish(Action::Close);
        assert_eq!(r.image.map(|i| i.dimensions()), Some((100, 50)));
    }

    #[test]
    fn click_without_drag_then_escape_gives_no_image() {
        let mut s = session(false);
        drag(&mut s, (10, 10), (10, 10));
        assert_eq!(s.key(Key::Escape), Outcome::Finish(Action::Close));
        assert!(s.finish(Action::Close).image.is_none());
    }

    #[test]
    fn copy_needs_a_selection() {
        let mut s = session(false);
        assert_eq!(s.key(Key::Copy), Outcome::Nothing);
        drag(&mut s, (10, 10), (60, 60));
        assert_eq!(s.key(Key::Copy), Outcome::Finish(Action::Copy));
    }

    #[test]
    fn drawing_with_a_tool_and_undo() {
        let mut s = session(false);
        drag(&mut s, (-100, 0), (100, 100));
        s.tool = Some(Tool::Rect);
        drag(&mut s, (-50, 20), (50, 80));
        drag(&mut s, (0, 50), (0, 50)); // a click draws nothing
        assert_eq!(s.ann.shapes().len(), 1);
        assert_eq!(s.key(Key::Undo), Outcome::Redraw);
        assert!(s.ann.is_empty());
    }

    #[test]
    fn typing_text_then_escape_cancels_only_the_text() {
        let mut s = session(true);
        drag(&mut s, (0, 0), (150, 90));
        s.tool = Some(Tool::Text);
        drag(&mut s, (20, 40), (20, 40));
        assert!(s.editing());
        s.key(Key::Text("hi".into()));
        assert_eq!(s.key(Key::Escape), Outcome::Redraw);
        assert!(s.ann.is_empty() && !s.editing());
        s.moved(PxPoint { x: 30, y: 40 });
        s.pressed();
        s.key(Key::Text("ok".into()));
        s.key(Key::Enter);
        assert!(matches!(&s.ann.shapes()[0], Shape::Text { text, .. } if text == "ok"));
    }

    #[test]
    fn resize_never_loses_the_selection() {
        let mut s = session(false);
        drag(&mut s, (0, 0), (100, 50));
        drag(&mut s, (100, 25), (1, 25));
        assert!(s.sel.rect().is_some());
    }
}
