//! The value field — "how good is it to hold the soul at a position?"
//!
//! One continuous surface over the whole arena, the shared currency every
//! decision spends (carry / pass / — later — off-ball drift and defense). The
//! base is **basketball-shaped**: a layup is the best shot, so value peaks at
//! your own goal and decays with distance, scaled by how *open* the spot is.
//!
//! ```text
//! value(P) = closeness(P, own_goal) × openness(P, enemies)
//! ```
//!
//! Borrowed in spirit from the Mitts utility AI, but reshaped for our rules and
//! kept **shallow** (a pointwise function, no lookahead/search — design laws
//! §2/§10) and **fixed-point / trig-free** (distances and projections, never
//! angles). Role re-weighting and the Finishing gate plug in here later; for now
//! every agent reads the same base field.

use crate::fx::{Fx, Vec2, WideFx};
use crate::world::SimConfig;

/// Value of holding the soul at `pos`, for a team whose own goal is `own_goal`.
pub fn value_at(pos: Vec2, own_goal: Vec2, enemies: &[Vec2], config: &SimConfig) -> Fx {
    closeness(pos, own_goal, config) * openness(pos, enemies, config)
}

/// Closeness to the goal in `[0, 1]`: 1 at the goal (the layup), ramping to 0 at
/// `value_span`. The base "straight to the goal is best, all else equal".
pub fn closeness(pos: Vec2, own_goal: Vec2, config: &SimConfig) -> Fx {
    let dist = pos.distance_to(own_goal);
    if dist >= config.value_span {
        Fx::from_num(0)
    } else {
        Fx::from_num(1) - dist / config.value_span
    }
}

/// Openness in `[0, 1]`: 1 with no enemies near, falling as defenders crowd the
/// spot. A distance-weighted count of enemies within `pressure_radius`,
/// normalized by `pressure_max` (the count that fully smothers a spot).
pub fn openness(pos: Vec2, enemies: &[Vec2], config: &SimConfig) -> Fx {
    let radius = config.pressure_radius;
    let mut pressure = Fx::from_num(0);
    for &enemy in enemies {
        let dist = pos.distance_to(enemy);
        if dist < radius {
            pressure += Fx::from_num(1) - dist / radius;
        }
    }
    let normalized = (pressure / config.pressure_max).min(Fx::from_num(1));
    Fx::from_num(1) - normalized
}

/// Pass-lane clearance in `[0, 1]`: 1 if no enemy sits in the `from`→`to` line,
/// falling toward 0 as a defender nears the segment. Single-blocker model — the
/// worst-placed defender defines the lane (max block, not a sum).
pub fn lane_clear(from: Vec2, to: Vec2, enemies: &[Vec2], config: &SimConfig) -> Fx {
    let segment = to - from;
    let len_sq: WideFx = segment.x.wide_mul(segment.x) + segment.y.wide_mul(segment.y);
    let zero = WideFx::from_num(0);
    if len_sq <= zero {
        return Fx::from_num(1); // degenerate (overlapping endpoints)
    }

    let radius = config.lane_radius;
    let mut max_block = Fx::from_num(0);
    for &enemy in enemies {
        let offset = enemy - from;
        // Projection of the enemy onto the segment, as t·len_sq (avoids a divide
        // until we know the enemy is actually between the endpoints).
        let dot: WideFx = offset.x.wide_mul(segment.x) + offset.y.wide_mul(segment.y);
        if dot <= zero || dot >= len_sq {
            continue; // behind `from` or past `to`
        }
        let t = Fx::saturating_from_num(dot / len_sq);
        let closest = from + segment.scale(t);
        let perp = enemy.distance_to(closest);
        if perp < radius {
            let block = Fx::from_num(1) - perp / radius;
            if block > max_block {
                max_block = block;
            }
        }
    }
    Fx::from_num(1) - max_block
}

/// How much a defender standing at `point` shadows the lane `a`→`b`: the block
/// factor in `[0, 1]` (perpendicular distance to the segment, endpoints
/// excluded). This is the "cover shadow" — a body in the lane reduces a pass's
/// completion. Mirrors the per-defender block used in pass completion.
pub fn segment_shadow(point: Vec2, a: Vec2, b: Vec2, lane_radius: Fx) -> Fx {
    let seg = b - a;
    let len_sq: WideFx = seg.x.wide_mul(seg.x) + seg.y.wide_mul(seg.y);
    let zero = WideFx::from_num(0);
    if len_sq <= zero {
        return Fx::from_num(0);
    }
    let off = point - a;
    let dot: WideFx = off.x.wide_mul(seg.x) + off.y.wide_mul(seg.y);
    if dot <= zero || dot >= len_sq {
        return Fx::from_num(0); // not between the endpoints — casts no shadow
    }
    let t = Fx::saturating_from_num(dot / len_sq);
    let perp = point.distance_to(a + seg.scale(t));
    if perp >= lane_radius {
        Fx::from_num(0)
    } else {
        Fx::from_num(1) - perp / lane_radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SimConfig {
        SimConfig::default()
    }

    fn v(x: i32, y: i32) -> Vec2 {
        Vec2::new(Fx::from_num(x), Fx::from_num(y))
    }

    #[test]
    fn closeness_peaks_at_the_goal() {
        let goal = v(-45, 0);
        assert!(closeness(goal, goal, &cfg()) > closeness(v(0, 0), goal, &cfg()));
    }

    #[test]
    fn openness_drops_with_a_nearby_enemy() {
        let spot = v(0, 0);
        let open = openness(spot, &[], &cfg());
        let crowded = openness(spot, &[v(1, 0)], &cfg());
        assert_eq!(open, Fx::from_num(1));
        assert!(crowded < open);
    }

    #[test]
    fn lane_is_clear_with_no_enemies() {
        assert_eq!(lane_clear(v(0, 0), v(20, 0), &[], &cfg()), Fx::from_num(1));
    }

    #[test]
    fn lane_is_blocked_by_a_defender_on_the_line() {
        let blocked = lane_clear(v(0, 0), v(20, 0), &[v(10, 0)], &cfg());
        let clear = lane_clear(v(0, 0), v(20, 0), &[v(10, 30)], &cfg());
        assert!(blocked < Fx::from_num(1));
        assert_eq!(clear, Fx::from_num(1)); // defender far off the line
    }
}
