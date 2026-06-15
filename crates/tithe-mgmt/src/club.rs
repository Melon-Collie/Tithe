//! Clubs and their tactics. A [`Club`] is a roster of persistent [`Player`]s
//! plus the coach's [`Tactics`]; [`Club::to_team_setup`] projects the two into
//! the sim's input form (a [`TeamSetup`] + its named formations).

use crate::player::Player;
use serde::{Deserialize, Serialize};
use tithe_sim::setup::{FormationSpec, TeamSetup};
use tithe_sim::{InPossessionRole as IP, OutOfPossessionRole as OP, PlayerSetup, Rng};

/// The coach's two inputs made concrete (CLAUDE.md — the entire input space):
/// the **positioning templates** (an in-possession and an out-of-possession
/// formation) and the **role assignment** (`roles[i]` is the in/out-of-possession
/// casting for the player in slot `i`). Player `i` fills slot `i` of both shapes,
/// so `roles` and the roster must match the formations' slot count.
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
                (IP::Runner, OP::Marker),
                (IP::Pivot, OP::Hawk),
                (IP::Playmaker, OP::Marker),
                (IP::Runner, OP::Destroyer),
                (IP::Finisher, OP::Cheat),
                (IP::Pivot, OP::Destroyer),
            ],
        }
    }
}

/// A club: a display name, its roster (in slot order), and its [`Tactics`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Club {
    pub name: String,
    pub players: Vec<Player>,
    pub tactics: Tactics,
}

impl Club {
    /// Project this club into the sim's input form: a [`TeamSetup`] (referencing
    /// its formations by `{key}_attack` / `{key}_defend`) and the two named
    /// [`FormationSpec`]s the caller must register in the [`MatchSetup`]. `key`
    /// namespaces the formations so two clubs' shapes can't collide in the shared
    /// formation library.
    ///
    /// [`MatchSetup`]: tithe_sim::MatchSetup
    pub fn to_team_setup(&self, key: &str) -> (TeamSetup, Vec<(String, FormationSpec)>) {
        let attack_name = format!("{key}_attack");
        let defend_name = format!("{key}_defend");

        let players = self
            .players
            .iter()
            .zip(&self.tactics.roles)
            .map(|(p, &(attack_role, defend_role))| {
                let r = &p.ratings;
                PlayerSetup {
                    name: p.name.clone(),
                    attack_role,
                    defend_role,
                    accuracy: r.accuracy,
                    range: r.range,
                    handling: r.handling,
                    stripping: r.stripping,
                    contesting: r.contesting,
                    passing: r.passing,
                    positioning: r.positioning,
                    pace: r.pace,
                    awareness: r.awareness,
                    endurance: r.endurance,
                }
            })
            .collect();

        let team = TeamSetup {
            name: self.name.clone(),
            attack_formation: attack_name.clone(),
            defend_formation: defend_name.clone(),
            players,
        };
        let formations = vec![
            (attack_name, self.tactics.attack_formation.clone()),
            (defend_name, self.tactics.defend_formation.clone()),
        ];
        (team, formations)
    }
}

/// Generate a seven-a-side club: seven players with ids drawn from the shared
/// `next_id` counter (so ids stay globally unique across the career), and the
/// default tactics. Deterministic from the threaded [`Rng`].
pub(crate) fn generate_club(name: &str, prefix: char, next_id: &mut u32, rng: &mut Rng) -> Club {
    use crate::player::PlayerId;
    let players = (0..7)
        .map(|i| {
            let id = PlayerId(*next_id);
            *next_id += 1;
            Player::generate(id, &format!("{prefix}{}", i + 1), rng)
        })
        .collect();
    Club {
        name: name.to_string(),
        players,
        tactics: Tactics::default_seven(),
    }
}
