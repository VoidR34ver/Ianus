//! Axis-aligned hitboxes, in integer subunits.
//!
//! A box covers `[min, max)` on each axis: the minimum edge is inside, the
//! maximum edge is outside. Two boxes that only share an edge do **not**
//! overlap, which is what makes a body resting flush against a wall count as
//! "touching but not inside".
//!
//! The overlap test is four integer comparisons combined with a bitwise AND
//! (not `&&`), so there is no branch to mispredict: it compiles to straight
//! line code, and a whole batch of tests can be vectorised. It is exact for
//! non-empty boxes; see [`Aabb::overlaps`] for the one precondition.

/// A hitbox: `[min_x, max_x) x [min_y, max_y)` in subunits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Aabb {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min_x: 0,
        min_y: 0,
        max_x: 0,
        max_y: 0,
    };

    pub const fn new(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// A box centred on `(cx, cy)` with half-width `hx` and half-height `hy`.
    /// Saturates instead of overflowing, so a body at the edge of the world
    /// still gets a valid box.
    pub const fn from_center(cx: i32, cy: i32, hx: i32, hy: i32) -> Self {
        Self {
            min_x: cx.saturating_sub(hx),
            min_y: cy.saturating_sub(hy),
            max_x: cx.saturating_add(hx),
            max_y: cy.saturating_add(hy),
        }
    }

    /// Do the two boxes share any area? Boxes that merely touch along an
    /// edge do not overlap.
    ///
    /// Exact when **both boxes are non-empty** (see [`is_empty`](Self::is_empty)).
    /// That precondition is what keeps this to four comparisons: an empty box
    /// lying inside another would otherwise count as overlapping. Everything
    /// in this crate that stores or probes boxes ([`crate::BroadPhase`],
    /// [`crate::Solids`]) skips empty boxes itself, so only code calling this
    /// directly needs to care. Use [`overlaps_checked`](Self::overlaps_checked)
    /// if a box might be empty.
    #[inline(always)]
    pub const fn overlaps(&self, other: &Aabb) -> bool {
        (self.min_x < other.max_x)
            & (other.min_x < self.max_x)
            & (self.min_y < other.max_y)
            & (other.min_y < self.max_y)
    }

    /// [`overlaps`](Self::overlaps), but safe for boxes that may be empty:
    /// empty boxes overlap nothing.
    #[inline(always)]
    pub const fn overlaps_checked(&self, other: &Aabb) -> bool {
        self.overlaps(other) & !self.is_empty() & !other.is_empty()
    }

    /// Zero or negative width or height.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        !((self.min_x < self.max_x) & (self.min_y < self.max_y))
    }

    /// Is the point inside the box?
    #[inline(always)]
    pub const fn contains_point(&self, x: i32, y: i32) -> bool {
        (self.min_x <= x) & (x < self.max_x) & (self.min_y <= y) & (y < self.max_y)
    }

    /// The same box moved by `(dx, dy)`, saturating.
    pub const fn translated(&self, dx: i32, dy: i32) -> Aabb {
        Aabb {
            min_x: self.min_x.saturating_add(dx),
            min_y: self.min_y.saturating_add(dy),
            max_x: self.max_x.saturating_add(dx),
            max_y: self.max_y.saturating_add(dy),
        }
    }

    pub const fn width(&self) -> i32 {
        self.max_x.saturating_sub(self.min_x)
    }

    pub const fn height(&self) -> i32 {
        self.max_y.saturating_sub(self.min_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x0: i32, y0: i32, x1: i32, y1: i32) -> Aabb {
        Aabb::new(x0, y0, x1, y1)
    }

    #[test]
    fn overlapping_boxes_overlap_in_both_directions() {
        let (a, c) = (b(0, 0, 10, 10), b(5, 5, 15, 15));
        assert!(a.overlaps(&c) && c.overlaps(&a));
    }

    #[test]
    fn boxes_that_only_share_an_edge_do_not_overlap() {
        let a = b(0, 0, 10, 10);
        assert!(!a.overlaps(&b(10, 0, 20, 10)), "right edge");
        assert!(!a.overlaps(&b(-10, 0, 0, 10)), "left edge");
        assert!(!a.overlaps(&b(0, 10, 10, 20)), "top edge");
        assert!(!a.overlaps(&b(0, -10, 10, 0)), "bottom edge");
        assert!(!a.overlaps(&b(10, 10, 20, 20)), "corner");
    }

    #[test]
    fn one_subunit_of_overlap_counts() {
        assert!(b(0, 0, 10, 10).overlaps(&b(9, 9, 20, 20)));
    }

    #[test]
    fn containment_and_identity() {
        let a = b(0, 0, 10, 10);
        assert!(a.overlaps(&b(2, 2, 3, 3)));
        assert!(b(2, 2, 3, 3).overlaps(&a));
        assert!(a.overlaps(&a));
    }

    #[test]
    fn the_checked_test_makes_empty_boxes_overlap_nothing() {
        let empty = b(5, 5, 5, 9);
        let big = b(0, 0, 100, 100);
        assert!(empty.is_empty());
        assert!(!empty.overlaps_checked(&big));
        assert!(!big.overlaps_checked(&empty));
        assert!(!empty.overlaps_checked(&empty));
        assert!(b(3, 3, 2, 9).is_empty(), "negative size is empty too");
        assert!(
            big.overlaps_checked(&b(1, 1, 2, 2)),
            "and it still works for real boxes"
        );
        // The unchecked fast test documents why the checked one exists.
        assert!(
            empty.overlaps(&big),
            "precondition: unchecked overlaps needs non-empty boxes"
        );
    }

    #[test]
    fn from_center_saturates_at_the_edge_of_the_world() {
        let a = Aabb::from_center(i32::MAX - 1, i32::MIN + 1, 100, 100);
        assert_eq!(a.max_x, i32::MAX);
        assert_eq!(a.min_y, i32::MIN);
        assert!(!a.is_empty());
    }

    #[test]
    fn point_containment_is_half_open() {
        let a = b(0, 0, 10, 10);
        assert!(a.contains_point(0, 0) && a.contains_point(9, 9));
        assert!(!a.contains_point(10, 5) && !a.contains_point(5, 10) && !a.contains_point(-1, 5));
    }

    #[test]
    fn translation_and_size() {
        let a = b(0, 0, 4, 6).translated(10, -2);
        assert_eq!(a, b(10, -2, 14, 4));
        assert_eq!((a.width(), a.height()), (4, 6));
    }

    /// The branchless test must agree with the obvious definition (the
    /// intersection rectangle has positive area) for every pair of non-empty
    /// boxes, at every boundary. Exhaustive over a small set of edge values.
    #[test]
    fn matches_the_obvious_definition_exhaustively_for_non_empty_boxes() {
        let vals = [-3, -1, 0, 1, 2, 4];
        let mut checked = 0;
        for &ax0 in &vals {
            for &ax1 in &vals {
                for &ay0 in &vals {
                    for &ay1 in &vals {
                        let a = b(ax0, ay0, ax1, ay1);
                        for &bx0 in &vals {
                            for &bx1 in &vals {
                                // y edges reuse the same value set, shifted by
                                // pairing, to keep the count manageable.
                                for (by0, by1) in [(bx0, bx1), (bx1, bx0), (-1, 2), (0, 1)] {
                                    let c = b(bx0, by0, bx1, by1);
                                    if a.is_empty() || c.is_empty() {
                                        continue;
                                    }
                                    let ox = a.min_x.max(c.min_x) < a.max_x.min(c.max_x);
                                    let oy = a.min_y.max(c.min_y) < a.max_y.min(c.max_y);
                                    assert_eq!(a.overlaps(&c), ox && oy, "{a:?} vs {c:?}");
                                    assert_eq!(a.overlaps(&c), c.overlaps(&a), "symmetry");
                                    assert_eq!(a.overlaps(&c), a.overlaps_checked(&c));
                                    checked += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 10_000, "only {checked} cases ran");
    }
}
