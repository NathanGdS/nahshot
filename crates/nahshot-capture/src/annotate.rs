//! Annotation model: shapes drawn over a selection.
//!
//! Coordinates are relative to the selection's top-left corner. Thickness
//! constants are in logical pixels; whoever creates a shape multiplies them by
//! the scale of the monitor where the shape started.

/// Stroke width for pen, line, arrow, rectangle and ellipse.
pub const STROKE_WIDTH: f32 = 3.0;
/// Highlighter marker width.
pub const MARKER_WIDTH: f32 = 18.0;
/// Highlighter marker opacity.
pub const MARKER_ALPHA: f32 = 0.35;
/// Text size.
pub const TEXT_SIZE: f32 = 20.0;

/// Annotation tool selected in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Marker,
    Text,
}

/// An opaque RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

/// Toolbar palette: red (default), orange, yellow, green, blue, purple, black, white.
pub const PALETTE: [Color; 8] = [
    Color(0xE5, 0x39, 0x35),
    Color(0xFB, 0x8C, 0x00),
    Color(0xFD, 0xD8, 0x35),
    Color(0x43, 0xA0, 0x47),
    Color(0x1E, 0x88, 0xE5),
    Color(0x8E, 0x24, 0xAA),
    Color(0x00, 0x00, 0x00),
    Color(0xFF, 0xFF, 0xFF),
];

/// A point relative to the selection corner.
pub type Pt = (f32, f32);

/// A drawn annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Pen {
        pts: Vec<Pt>,
        color: Color,
        width: f32,
    },
    Line {
        a: Pt,
        b: Pt,
        color: Color,
        width: f32,
    },
    Arrow {
        a: Pt,
        b: Pt,
        color: Color,
        width: f32,
    },
    Rect {
        a: Pt,
        b: Pt,
        color: Color,
        width: f32,
    },
    Ellipse {
        a: Pt,
        b: Pt,
        color: Color,
        width: f32,
    },
    Marker {
        pts: Vec<Pt>,
        color: Color,
    },
    Text {
        at: Pt,
        text: String,
        color: Color,
        size: f32,
    },
}

/// The ordered list of shapes on a selection, with undo.
#[derive(Debug, Default)]
pub struct Annotations {
    shapes: Vec<Shape>,
}

impl Annotations {
    /// Adds a shape on top.
    pub fn push(&mut self, shape: Shape) {
        self.shapes.push(shape);
    }

    /// Removes and returns the most recent shape.
    pub fn undo(&mut self) -> Option<Shape> {
        self.shapes.pop()
    }

    /// The topmost shape, for editing a shape while it is being drawn.
    pub fn last_mut(&mut self) -> Option<&mut Shape> {
        self.shapes.last_mut()
    }

    /// Shapes in drawing order.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// Whether there are no shapes.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_pops_last_shape() {
        let mut a = Annotations::default();
        assert!(a.is_empty());
        a.push(Shape::Line {
            a: (0.0, 0.0),
            b: (1.0, 1.0),
            color: PALETTE[0],
            width: STROKE_WIDTH,
        });
        a.push(Shape::Marker {
            pts: vec![(0.0, 0.0)],
            color: PALETTE[2],
        });
        assert_eq!(a.shapes().len(), 2);
        assert!(matches!(a.undo(), Some(Shape::Marker { .. })));
        assert_eq!(a.shapes().len(), 1);
        assert!(a.undo().is_some());
        assert!(a.undo().is_none());
        assert!(a.is_empty());
    }

    #[test]
    fn last_mut_edits_the_shape_in_progress() {
        let mut a = Annotations::default();
        assert!(a.last_mut().is_none());
        a.push(Shape::Pen {
            pts: vec![(0.0, 0.0)],
            color: PALETTE[0],
            width: STROKE_WIDTH,
        });
        if let Some(Shape::Pen { pts, .. }) = a.last_mut() {
            pts.push((5.0, 5.0));
        }
        assert!(matches!(&a.shapes()[0], Shape::Pen { pts, .. } if pts.len() == 2));
    }
}
