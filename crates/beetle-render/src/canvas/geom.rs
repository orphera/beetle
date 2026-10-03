//! Minimal 2D geometry for the Canvas API.

/// Axis-aligned rectangle in screen pixels (origin top-left, y down).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Shrinks every edge by `d` (negative grows). Never returns negative size.
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_by(Insets::uniform(d))
    }

    pub fn inset_by(&self, i: Insets) -> Rect {
        Rect::new(
            self.x + i.left,
            self.y + i.top,
            (self.w - i.left - i.right).max(0.0),
            (self.h - i.top - i.bottom).max(0.0),
        )
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let l = self.x.max(other.x);
        let t = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > l && b > t).then(|| Rect::from_ltrb(l, t, r, b))
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

/// Per-edge distances (padding, 9-slice borders).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn uniform(d: f32) -> Self {
        Self::new(d, d, d, d)
    }

    pub fn scaled(&self, s: f32) -> Self {
        Self::new(self.left * s, self.top * s, self.right * s, self.bottom * s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_and_inset() {
        let a = Rect::new(0.0, 0.0, 100.0, 50.0);
        let b = Rect::new(80.0, 40.0, 50.0, 50.0);
        assert_eq!(a.intersect(&b), Some(Rect::new(80.0, 40.0, 20.0, 10.0)));
        assert_eq!(a.intersect(&Rect::new(200.0, 0.0, 1.0, 1.0)), None);
        assert_eq!(a.inset(10.0), Rect::new(10.0, 10.0, 80.0, 30.0));
        assert_eq!(a.inset(40.0).h, 0.0);
    }
}
