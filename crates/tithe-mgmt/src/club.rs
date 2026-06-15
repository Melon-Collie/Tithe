//! Clubs and their tactics. A [`Club`] does **not** own its players — it holds a
//! roster of [`PlayerId`]s into the career's central player pool (so a player can
//! exist without a club: a free agent). The projection from a club to the sim's
//! input lives on [`crate::Career`], since it needs the pool to resolve those ids.

use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use tithe_sim::setup::FormationSpec;
use tithe_sim::{InPossessionRole as IP, OutOfPossessionRole as OP};

/// The coach's two inputs made concrete (CLAUDE.md — the entire input space):
/// the **positioning templates** (an in-possession and an out-of-possession
/// formation) and the **role assignment** (`roles[i]` is the in/out-of-possession
/// casting for the player in slot `i`). The player in roster slot `i` fills slot
/// `i` of both shapes, so `roles` and the roster must match the formations' slot
/// count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tactics {
    pub attack_formation: FormationSpec,
    pub defend_formation: FormationSpec,
    pub roles: Vec<(IP, OP)>,
}

impl Tactics {
    /// A sensible default for a seven-a-side club: a high attacking push and a
    /// deep defending block, with a keeper/back/mid/forward role spread. Mirrors
    /// the sim's `default_match` shapes so a generated career plays a familiar
    /// game out of the box. Slots are axial hexes `[q, r]` (§14).
    pub fn default_seven() -> Self {
        Tactics {
            attack_formation: FormationSpec {
                slots: vec![
                    [3, 0],  // 0 anchor (deepest safety, steps up)
                    [0, -2], // 1 rover
                    [-2, 2], // 2 playmaker (outlet)
                    [-1, 0], // 3 rover
                    [3, -3], // 4 presser (wide support)
                    [-5, 0], // 5 finisher (at the offering spot)
                    [0, 3],  // 6 presser (wide support)
                ],
            },
            defend_formation: FormationSpec {
                slots: vec![
                    [5, 0],  // 0 anchor (last line at the defended goal)
                    [4, -2], // 1 rover
                    [0, 2],  // 2 playmaker
                    [3, 0],  // 3 rover
                    [6, -3], // 4 presser (harries the buildup)
                    [-1, 0], // 5 finisher (stays high, ready to counter)
                    [3, 3],  // 6 presser
                ],
            },
            roles: vec![
                (IP::Outlet, OP::Sweeper),
                (IP::Roamer, OP::Warden),
                (IP::Playmaker, OP::Tracker),
                (IP::BoxToBox, OP::Warden),
                (IP::Roamer, OP::Presser),
                (IP::Finisher, OP::Cheat),
                (IP::Roamer, OP::Destroyer),
            ],
        }
    }
}

/// A club: a display name, its **roster** (player ids into the career pool, in
/// slot order), and its [`Tactics`]. The roster length must match the tactics'
/// slot count, or a match built from it will fail to assemble.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Club {
    pub name: String,
    pub roster: Vec<PlayerId>,
    pub tactics: Tactics,
}
