//! The hex board — the spatial substrate for the §14 redesign.
//!
//! An **axial** hex grid (pointy-top), clipped to an **oval** playable region:
//! the board is every hex whose center falls inside an ellipse, so it reads as a
//! rounded stadium rather than a rectangle. Hex coordinates are integers and
//! centers are fixed-point ([`Vec2`]), so the whole thing is bit-stable — the
//! `√3` factor is a fixed constant, not a runtime float (CLAUDE.md → determinism).
//!
//! Phase 1 of the redesign only *establishes and renders* this board; the sim's
//! decisions still run on continuous positions. Footprints (Phase 2) are sets of
//! [`Hex`]es placed on it.

use crate::fx::{Fx, Vec2};

/// √3 in fixed-point — the pointy-top hex geometry constant. Built from in-range
/// integers (Q16.16 caps at ±32767), and identical on every platform (no runtime
/// float). 1.7320 vs the true 1.7320508 is well under Q16.16's resolution.
fn sqrt3() -> Fx {
    Fx::from_num(17_320) / Fx::from_num(10_000) // 1.7320
}

/// An axial hex coordinate (pointy-top: `q` is the column axis, `r` the row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

/// The six axial neighbor directions.
const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

impl Hex {
    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Grid distance to another hex (cube distance) — the number of steps.
    pub fn distance_to(self, other: Hex) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        (dq.abs() + (dq + dr).abs() + dr.abs()) / 2
    }

    /// The six adjacent hexes (in `DIRECTIONS` order — deterministic).
    pub fn neighbors(self) -> [Hex; 6] {
        DIRECTIONS.map(|(dq, dr)| Hex::new(self.q + dq, self.r + dr))
    }
}

/// The pixel/field center of a hex, given the hex `size` (center-to-vertex).
/// Pointy-top: `x = size·√3·(q + r/2)`, `y = size·(3/2)·r`.
fn center_of(size: Fx, h: Hex) -> Vec2 {
    let half = Fx::from_num(1) / Fx::from_num(2);
    let q = Fx::from_num(h.q);
    let r = Fx::from_num(h.r);
    let x = size * sqrt3() * (q + r * half);
    let y = size * (Fx::from_num(3) * half) * r;
    Vec2::new(x, y)
}

/// A hex grid clipped to an oval (ellipse) — the playable board.
#[derive(Debug, Clone)]
pub struct Board {
    size: Fx,
    half_x: Fx,
    half_y: Fx,
    cells: Vec<Hex>,
}

impl Board {
    /// Build the oval board: every hex whose center lies inside the ellipse with
    /// the given half-extents, enumerated in a fixed (row-major) order so the
    /// cell list is deterministic.
    pub fn oval(size: Fx, half_x: Fx, half_y: Fx) -> Self {
        let one = Fx::from_num(1);
        // Generous bounding range; the ellipse test below clips to the oval.
        let col_span = (half_x / (size * sqrt3())).to_num::<i32>() + 2;
        let row_span = (half_y / (size * Fx::from_num(3) / Fx::from_num(2))).to_num::<i32>() + 2;
        let reach = col_span + row_span; // the r/2 shift widens the q range
        let mut cells = Vec::new();
        for r in -row_span..=row_span {
            for q in -reach..=reach {
                let h = Hex::new(q, r);
                let c = center_of(size, h);
                let rx = c.x / half_x;
                let ry = c.y / half_y;
                if rx * rx + ry * ry <= one {
                    cells.push(h);
                }
            }
        }
        Self {
            size,
            half_x,
            half_y,
            cells,
        }
    }

    /// The in-bounds hexes (deterministic order).
    pub fn cells(&self) -> &[Hex] {
        &self.cells
    }

    /// The hex size (center-to-vertex).
    pub fn size(&self) -> Fx {
        self.size
    }

    /// The field center of a hex.
    pub fn center(&self, h: Hex) -> Vec2 {
        center_of(self.size, h)
    }

    /// Whether a hex's center is inside the oval.
    pub fn contains(&self, h: Hex) -> bool {
        let c = center_of(self.size, h);
        let rx = c.x / self.half_x;
        let ry = c.y / self.half_y;
        rx * rx + ry * ry <= Fx::from_num(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_basics() {
        let o = Hex::new(0, 0);
        assert_eq!(o.distance_to(o), 0);
        for n in o.neighbors() {
            assert_eq!(o.distance_to(n), 1);
        }
        // Two steps along an axis.
        assert_eq!(Hex::new(0, 0).distance_to(Hex::new(2, 0)), 2);
    }

    #[test]
    fn center_of_origin_is_origin() {
        let size = Fx::from_num(4);
        assert_eq!(center_of(size, Hex::new(0, 0)), Vec2::default());
    }

    #[test]
    fn oval_board_is_clipped_and_centered() {
        let size = Fx::from_num(4);
        let half_x = Fx::from_num(50);
        let half_y = Fx::from_num(30);
        let board = Board::oval(size, half_x, half_y);
        assert!(!board.cells().is_empty());
        // The center hex is always in; a hex well outside the oval is not.
        assert!(board.contains(Hex::new(0, 0)));
        assert!(!board.contains(Hex::new(100, 0)));
        // Every listed cell genuinely passes the ellipse test, and the board is
        // wider than it is tall (x is the long axis).
        let one = Fx::from_num(1);
        let mut max_x = Fx::from_num(0);
        let mut max_y = Fx::from_num(0);
        for &h in board.cells() {
            let c = board.center(h);
            let rx = c.x / half_x;
            let ry = c.y / half_y;
            assert!(rx * rx + ry * ry <= one);
            max_x = max_x.max(c.x.abs());
            max_y = max_y.max(c.y.abs());
        }
        assert!(max_x > max_y);
    }
}
