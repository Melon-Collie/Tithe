//! Fixed-point math — the determinism foundation.
//!
//! The sim uses **Q16.16** fixed-point ([`Fx`]) for every continuous quantity
//! (position, velocity, stamina). It is integer-backed, so the same arithmetic
//! produces bit-identical results on native and WASM — unlike floats, which
//! drift across targets and would corrupt replays and shared-seed leagues
//! (CLAUDE.md → Determinism rules).
//!
//! `Fx` is a type alias over the `fixed` crate's `I16F16`, so swapping to a
//! wider format later (e.g. `I32F32`) is a one-line change here.
//!
//! Note on functions: `sqrt` is fine — it is exact integer math. Trigonometry
//! (`sin`/`cos`) is **not** and is deliberately absent; where the sim needs
//! distances it compares lengths, never angles.

use crate::rng::Rng;

/// The sim's fixed-point scalar: Q16.16 (16 integer bits, 16 fractional).
/// Range ≈ ±32768, resolution ≈ 1.5e-5.
pub type Fx = fixed::types::I16F16;

/// Double-width fixed-point (Q32.32), used for intermediate products so that
/// squaring a coordinate can't overflow [`Fx`] (see [`Vec2::length`]).
pub type WideFx = fixed::types::I32F32;

/// A 2D point/vector in fixed-point space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vec2 {
    pub x: Fx,
    pub y: Fx,
}

impl Vec2 {
    /// A vector from its components.
    pub fn new(x: Fx, y: Fx) -> Self {
        Self { x, y }
    }

    /// This vector scaled by a scalar.
    pub fn scale(self, k: Fx) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }

    /// Euclidean length.
    ///
    /// The squares are accumulated in [`WideFx`] via `wide_mul`, so a large
    /// coordinate (e.g. 200, whose square 40000 overflows Q16.16's ±32768)
    /// is handled without panicking. `sqrt` is deterministic integer math.
    pub fn length(self) -> Fx {
        let sum_of_squares: WideFx = self.x.wide_mul(self.x) + self.y.wide_mul(self.y);
        Fx::saturating_from_num(sum_of_squares.sqrt())
    }

    /// Distance from `self` to `other`.
    pub fn distance_to(self, other: Vec2) -> Fx {
        (self - other).length()
    }
}

/// Shortest distance from point `p` to the segment `a`–`b` (endpoints clamped).
/// Deterministic, fixed-point — the projection uses dot products, no trig.
pub fn point_to_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> Fx {
    let ab = b - a;
    let len_sq: WideFx = ab.x.wide_mul(ab.x) + ab.y.wide_mul(ab.y);
    let zero = WideFx::from_num(0);
    if len_sq <= zero {
        return p.distance_to(a); // degenerate segment
    }
    let ap = p - a;
    let dot: WideFx = ap.x.wide_mul(ab.x) + ap.y.wide_mul(ab.y);
    let closest = if dot <= zero {
        a
    } else if dot >= len_sq {
        b
    } else {
        a + ab.scale(Fx::saturating_from_num(dot / len_sq))
    };
    p.distance_to(closest)
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;

    /// Component-wise sum.
    fn add(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;

    /// Component-wise difference (`self - other`).
    fn sub(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x - other.x, self.y - other.y)
    }
}

/// A deterministic random point inside the box `[-half_x, half_x] ×
/// [-half_y, half_y]`, snapped to integer units. Threads the seeded [`Rng`].
pub fn random_point_in_box(rng: &mut Rng, half_x: Fx, half_y: Fx) -> Vec2 {
    Vec2::new(random_coord(rng, half_x), random_coord(rng, half_y))
}

/// A deterministic integer coordinate in `[-half, half]`.
fn random_coord(rng: &mut Rng, half: Fx) -> Fx {
    let h: i32 = half.to_num();
    let span = (2 * h + 1) as u64;
    let value = rng.below(span) as i64 - i64::from(h);
    Fx::from_num(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Fx, b: Fx) -> bool {
        let epsilon = Fx::from_num(1) / Fx::from_num(100); // 0.01
        (a - b).abs() <= epsilon
    }

    #[test]
    fn length_of_3_4_is_5() {
        let v = Vec2::new(Fx::from_num(3), Fx::from_num(4));
        assert!(close(v.length(), Fx::from_num(5)));
    }

    #[test]
    fn length_handles_large_coords_without_overflow() {
        // 200² = 40000 overflows Q16.16 (±32768); wide_mul keeps it exact.
        let v = Vec2::new(Fx::from_num(200), Fx::from_num(0));
        assert!(close(v.length(), Fx::from_num(200)));
    }

    #[test]
    fn point_to_segment_distance_basics() {
        let a = Vec2::new(Fx::from_num(0), Fx::from_num(0));
        let b = Vec2::new(Fx::from_num(10), Fx::from_num(0));
        // Off the middle of the segment: perpendicular distance.
        assert!(close(
            point_to_segment_distance(Vec2::new(Fx::from_num(5), Fx::from_num(3)), a, b),
            Fx::from_num(3)
        ));
        // Past an endpoint: distance to that endpoint.
        assert!(close(
            point_to_segment_distance(Vec2::new(Fx::from_num(13), Fx::from_num(0)), a, b),
            Fx::from_num(3)
        ));
    }

    #[test]
    fn random_coord_stays_in_bounds() {
        let mut rng = Rng::new(99);
        let half = Fx::from_num(50);
        for _ in 0..10_000 {
            let p = random_point_in_box(&mut rng, half, half);
            assert!(p.x >= -half && p.x <= half);
            assert!(p.y >= -half && p.y <= half);
        }
    }
}
