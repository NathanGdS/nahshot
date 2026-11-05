//! Selection geometry in physical pixels of the virtual desktop.

/// Smallest width/height (in pixels) for a selection to count.
pub const MIN_SELECTION: u32 = 3;

/// A point in virtual-desktop physical pixels (may be negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxPoint {
    pub x: i32,
    pub y: i32,
}

/// A rectangle in virtual-desktop physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl PxRect {
    /// One past the rightmost column.
    pub fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    /// One past the bottom row.
    pub fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }

    /// Whether `p` lies inside the rectangle (right/bottom edges exclusive).
    pub fn contains(&self, p: PxPoint) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// The overlapping area of two rectangles, if any.
    pub fn intersect(&self, other: &PxRect) -> Option<PxRect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| PxRect {
            x,
            y,
            w: (r - x) as u32,
            h: (b - y) as u32,
        })
    }
}

/// A grab point on a selection: the body (move) or one of eight resize handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

/// Selection edges (left, top, right, bottom), kept ordered; right and bottom are
/// one past the last selected column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Edges {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}

impl Edges {
    /// The pixels from `a` to `b`, both included, so a drag can reach the last
    /// column and row of the screen.
    fn spanning(a: PxPoint, b: PxPoint) -> Self {
        Edges {
            l: a.x.min(b.x),
            t: a.y.min(b.y),
            r: a.x.max(b.x) + 1,
            b: a.y.max(b.y) + 1,
        }
    }

    fn normalized(self) -> Self {
        Edges {
            l: self.l.min(self.r),
            t: self.t.min(self.b),
            r: self.l.max(self.r),
            b: self.t.max(self.b),
        }
    }
}

/// Pushes a dragged edge out of the band closer than [`MIN_SELECTION`] to the fixed edge,
/// on whichever side the drag put it, so a resize never makes the selection vanish.
fn keep_min_gap(moved: i32, fixed: i32) -> i32 {
    let min = MIN_SELECTION as i32;
    match moved - fixed {
        d if d.abs() >= min => moved,
        d if d < 0 => fixed - min,
        _ => fixed + min,
    }
}

/// The user's screenshot selection: created by dragging, then movable/resizable.
///
/// Resizes are applied to the edges as they are; to keep a drag continuous when an
/// edge crosses its opposite, clone the selection at drag start and re-apply each
/// move to that snapshot (the handle never needs swapping that way).
#[derive(Debug, Default, Clone)]
pub struct Selection {
    anchor: Option<PxPoint>,
    edges: Option<Edges>,
}

impl Selection {
    /// Starts a new selection at `p`, discarding any previous one.
    pub fn begin(&mut self, p: PxPoint) {
        self.anchor = Some(p);
        self.edges = Some(Edges::spanning(p, p));
    }

    /// Extends the initial drag from the `begin` point to `p`.
    pub fn update(&mut self, p: PxPoint) {
        if let Some(a) = self.anchor {
            self.edges = Some(Edges::spanning(a, p));
        }
    }

    /// The normalized selection, or `None` if smaller than [`MIN_SELECTION`].
    pub fn rect(&self) -> Option<PxRect> {
        let e = self.edges?;
        let (w, h) = ((e.r - e.l) as u32, (e.b - e.t) as u32);
        (w >= MIN_SELECTION && h >= MIN_SELECTION).then_some(PxRect {
            x: e.l,
            y: e.t,
            w,
            h,
        })
    }

    /// Which handle (if any) is under `p`, with `tolerance` pixels of slack on edges.
    pub fn hit(&self, p: PxPoint, tolerance: i32) -> Option<Handle> {
        let r = self.rect()?;
        let (l, t, rt, b) = (r.x, r.y, r.right(), r.bottom());
        let near = |a: i32, b: i32| (a - b).abs() <= tolerance;
        let (nl, nr, nt, nb) = (near(p.x, l), near(p.x, rt), near(p.y, t), near(p.y, b));
        let in_x = p.x >= l - tolerance && p.x <= rt + tolerance;
        let in_y = p.y >= t - tolerance && p.y <= b + tolerance;
        match () {
            _ if nl && nt => Some(Handle::NW),
            _ if nr && nt => Some(Handle::NE),
            _ if nl && nb => Some(Handle::SW),
            _ if nr && nb => Some(Handle::SE),
            _ if nt && in_x => Some(Handle::N),
            _ if nb && in_x => Some(Handle::S),
            _ if nl && in_y => Some(Handle::W),
            _ if nr && in_y => Some(Handle::E),
            _ if r.contains(p) => Some(Handle::Move),
            _ => None,
        }
    }

    /// Moves or resizes by the delta `to - from`; crossing an edge flips and normalizes.
    pub fn drag(&mut self, handle: Handle, from: PxPoint, to: PxPoint) {
        let Some(mut e) = self.edges else { return };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        if handle == Handle::Move {
            e = Edges {
                l: e.l + dx,
                t: e.t + dy,
                r: e.r + dx,
                b: e.b + dy,
            };
        } else {
            if matches!(handle, Handle::W | Handle::NW | Handle::SW) {
                e.l = keep_min_gap(e.l + dx, e.r);
            }
            if matches!(handle, Handle::E | Handle::NE | Handle::SE) {
                e.r = keep_min_gap(e.r + dx, e.l);
            }
            if matches!(handle, Handle::N | Handle::NW | Handle::NE) {
                e.t = keep_min_gap(e.t + dy, e.b);
            }
            if matches!(handle, Handle::S | Handle::SW | Handle::SE) {
                e.b = keep_min_gap(e.b + dy, e.t);
            }
        }
        self.edges = Some(e.normalized());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_without_drag_is_no_selection() {
        let mut s = Selection::default();
        s.begin(PxPoint { x: 10, y: 10 });
        s.update(PxPoint { x: 11, y: 11 });
        assert_eq!(s.rect(), None);
    }

    #[test]
    fn drag_any_direction_normalizes() {
        let mut s = Selection::default();
        s.begin(PxPoint { x: 100, y: 100 });
        s.update(PxPoint { x: -20, y: 40 });
        assert_eq!(
            s.rect(),
            Some(PxRect {
                x: -20,
                y: 40,
                w: 121,
                h: 61
            })
        );
    }

    #[test]
    fn hit_and_resize_handles() {
        let mut s = Selection::default();
        s.begin(PxPoint { x: 0, y: 0 });
        s.update(PxPoint { x: 100, y: 100 });
        let p = |x, y| PxPoint { x, y };
        assert_eq!(s.hit(p(100, 100), 4), Some(Handle::SE));
        assert_eq!(s.hit(p(50, 50), 4), Some(Handle::Move));
        assert_eq!(s.hit(p(300, 300), 4), None);

        s.drag(Handle::SE, p(100, 100), p(150, 120));
        assert_eq!(
            s.rect(),
            Some(PxRect {
                x: 0,
                y: 0,
                w: 151,
                h: 121
            })
        );
        s.drag(Handle::Move, p(50, 50), p(60, 70));
        assert_eq!(
            s.rect(),
            Some(PxRect {
                x: 10,
                y: 20,
                w: 151,
                h: 121
            })
        );
        s.drag(Handle::W, p(10, 50), p(200, 50));
        assert_eq!(
            s.rect(),
            Some(PxRect {
                x: 161,
                y: 20,
                w: 39,
                h: 121
            })
        );
    }

    fn square_0_100() -> Selection {
        let mut s = Selection::default();
        s.begin(PxPoint { x: 0, y: 0 });
        s.update(PxPoint { x: 100, y: 100 });
        s
    }

    #[test]
    fn resize_never_shrinks_below_min() {
        let mut s = square_0_100();
        s.drag(
            Handle::SE,
            PxPoint { x: 100, y: 100 },
            PxPoint { x: 1, y: 2 },
        );
        let r = s.rect().expect("selection survives the resize");
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, MIN_SELECTION, MIN_SELECTION));
    }

    #[test]
    fn resize_from_snapshot_crosses_opposite_edge_continuously() {
        let start = square_0_100();
        let from = PxPoint { x: 0, y: 50 };
        let drag_to = |x| {
            let mut s = start.clone();
            s.drag(Handle::W, from, PxPoint { x, y: 50 });
            s.rect().map(|r| (r.x, r.w))
        };
        assert_eq!(drag_to(50), Some((50, 51)));
        assert_eq!(drag_to(99), Some((98, 3)));
        assert_eq!(drag_to(101), Some((101, 3)));
        assert_eq!(drag_to(150), Some((101, 49)));
        assert_eq!(drag_to(160), Some((101, 59)));
    }

    #[test]
    fn dragged_edge_is_inclusive() {
        let drag = |to: PxPoint| {
            let mut s = Selection::default();
            s.begin(PxPoint { x: 0, y: 0 });
            s.update(to);
            s.rect().map(|r| (r.w, r.h))
        };
        // The pointer can reach the last pixel of a 1920x1080 screen, but not beyond.
        assert_eq!(drag(PxPoint { x: 1919, y: 1079 }), Some((1920, 1080)));
        assert_eq!(drag(PxPoint { x: 149, y: 99 }), Some((150, 100)));
    }
}
