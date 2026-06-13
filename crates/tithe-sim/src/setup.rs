//! The coach-input boundary: a serializable description of a matchup that the
//! sim turns into a starting roster.
//!
//! This is the concrete shape of §9's "serialized inputs go in." A
//! [`MatchSetup`] carries the **two coach inputs** plus the personnel they act
//! on (§7 — teams differ only by personnel):
//!
//! - a **library of named formations** (positioning templates); each team picks
//!   two — an `attack_formation` (in-possession) and a `defend_formation`
//!   (out-of-possession), so its shape morphs by phase (§1);
//! - per team, a **roster** of players, each with two roles (in/out-of-possession
//!   casting) and the attribute set;
//! - the **assignment** is positional: the *n*-th player fills the *n*-th slot
//!   of *both* of that team's shapes.
//!
//! It comes from a file today (a consumer parses TOML/JSON into this) and from
//! the game's UI later — the sim doesn't care which. **No floats and no Fx live
//! in the wire format:** attributes are authored as integer `0..=100` and
//! converted to fixed-point here, so the format stays human-authorable and the
//! determinism contract is untouched.
//!
//! Tuning config (`SimConfig` — arena size, souls-to-win, the AI dials) is *not*
//! part of the setup yet; a match built from a setup uses [`SimConfig::default`].
//!
//! [`SimConfig::default`]: crate::SimConfig

use crate::fx::{Fx, Vec2};
use crate::hex::Hex;
use crate::world::{Agent, Attributes, InPossessionRole, OutOfPossessionRole, SimConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A full matchup description: the formation library and the two teams.
///
/// `formations` is keyed by name (a `BTreeMap`, so iteration/serialization is
/// ordered — a determinism habit even though build-time access is by key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSetup {
    /// Named positioning templates; teams reference these by name.
    pub formations: BTreeMap<String, FormationSpec>,
    /// Exactly two teams (`[0]` attacks -x, `[1]` attacks +x).
    pub teams: Vec<TeamSetup>,
}

/// A positioning template: field-relative anchors, authored in the **team-0
/// frame** (own goal at -x, attacking +x). When team 1 uses a template it is
/// mirrored across x, so one named shape serves either side symmetrically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationSpec {
    /// One axial **hex** `[q, r]` per slot — the board cell the player anchors on
    /// (§14; players live on the grid). It resolves to that hex's field center.
    pub slots: Vec<[i32; 2]>,
}

/// One team: a display name, the two **phase formations** it fields (each by key
/// into [`MatchSetup::formations`]), and its players in slot order. Player *i*
/// fills slot *i* of **both** shapes, so the two formations must have the same
/// slot count as the roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSetup {
    pub name: String,
    /// Shape used while this team holds the soul (in-possession).
    pub attack_formation: String,
    /// Shape used while it doesn't (enemy-held or loose; out-of-possession).
    pub defend_formation: String,
    pub players: Vec<PlayerSetup>,
}

/// One authored player. Attributes are integer percentiles `0..=100`
/// (`accuracy` 80 = the Fx `0.80` the sim consumes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSetup {
    pub name: String,
    /// What he does with the soul (in-possession casting).
    #[serde(default)]
    pub attack_role: InPossessionRole,
    /// What he does without it (out-of-possession casting).
    #[serde(default)]
    pub defend_role: OutOfPossessionRole,
    pub accuracy: u8,
    pub range: u8,
    pub handling: u8,
    pub stripping: u8,
    pub contesting: u8,
    pub passing: u8,
    pub positioning: u8,
    pub pace: u8,
    pub awareness: u8,
}

impl PlayerSetup {
    /// Convert the authored percentiles to the sim's fixed-point [`Attributes`],
    /// validating each is in `0..=100`.
    fn to_attributes(&self) -> Result<Attributes, SetupError> {
        let one = |label: &'static str, v: u8| -> Result<Fx, SetupError> {
            if v > 100 {
                return Err(SetupError::AttributeOutOfRange {
                    player: self.name.clone(),
                    attribute: label,
                    value: v,
                });
            }
            Ok(Fx::from_num(v) / Fx::from_num(100))
        };
        Ok(Attributes {
            accuracy: one("accuracy", self.accuracy)?,
            range: one("range", self.range)?,
            handling: one("handling", self.handling)?,
            stripping: one("stripping", self.stripping)?,
            contesting: one("contesting", self.contesting)?,
            passing: one("passing", self.passing)?,
            positioning: one("positioning", self.positioning)?,
            pace: one("pace", self.pace)?,
            awareness: one("awareness", self.awareness)?,
        })
    }
}

/// Why a [`MatchSetup`] could not be turned into a roster. These are authoring
/// mistakes, surfaced with enough context to fix the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// A setup must field exactly two teams.
    WrongTeamCount(usize),
    /// A team referenced a formation name not in the library.
    FormationNotFound { team: String, formation: String },
    /// A team's roster size doesn't match its formation's slot count.
    PlayerCountMismatch {
        team: String,
        formation: String,
        players: usize,
        slots: usize,
    },
    /// An attribute was authored outside `0..=100`.
    AttributeOutOfRange {
        player: String,
        attribute: &'static str,
        value: u8,
    },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::WrongTeamCount(n) => {
                write!(f, "a match needs exactly 2 teams, found {n}")
            }
            SetupError::FormationNotFound { team, formation } => {
                write!(
                    f,
                    "team '{team}' references unknown formation '{formation}'"
                )
            }
            SetupError::PlayerCountMismatch {
                team,
                formation,
                players,
                slots,
            } => write!(
                f,
                "team '{team}' has {players} players but formation '{formation}' has {slots} slots"
            ),
            SetupError::AttributeOutOfRange {
                player,
                attribute,
                value,
            } => write!(
                f,
                "player '{player}' {attribute} = {value} is out of range (0..=100)"
            ),
        }
    }
}

impl std::error::Error for SetupError {}

/// Turn a validated [`MatchSetup`] into the starting agents (team 0 first, ids
/// sequential). Each player gets two phase anchors from the team's two
/// formations; both are mirrored across x for team 1. Agents start on the
/// **defending** anchor (the faceoff soul is loose → out-of-possession). Errors
/// point at the authoring mistake (see [`SetupError`]); no RNG is consumed —
/// attributes are authored, so a match is reproducible from the seed alone.
pub fn build_agents(setup: &MatchSetup) -> Result<Vec<Agent>, SetupError> {
    if setup.teams.len() != 2 {
        return Err(SetupError::WrongTeamCount(setup.teams.len()));
    }
    let mut agents = Vec::new();
    for (team_idx, team) in setup.teams.iter().enumerate() {
        let attack = lookup_formation(setup, team, &team.attack_formation)?;
        let defend = lookup_formation(setup, team, &team.defend_formation)?;
        check_slot_count(team, &team.attack_formation, attack)?;
        check_slot_count(team, &team.defend_formation, defend)?;

        let hex_size = SimConfig::default().hex_size;
        for (i, player) in team.players.iter().enumerate() {
            let attack_anchor = anchor_for(team_idx, &attack.slots[i], hex_size);
            let defend_anchor = anchor_for(team_idx, &defend.slots[i], hex_size);
            let attributes = player.to_attributes()?;
            let id = agents.len() as u32;
            agents.push(Agent {
                id,
                name: player.name.clone(),
                team: team_idx as u8,
                attack_role: player.attack_role,
                defend_role: player.defend_role,
                // Faceoff soul is loose → out-of-possession → defending shape.
                pos: defend_anchor,
                target: defend_anchor,
                anchor: defend_anchor,
                attack_anchor,
                defend_anchor,
                stagger: 0,
                stamina: Fx::from_num(1),
                attributes,
            });
        }
    }
    Ok(agents)
}

/// Look up one of a team's formations by name, or report it missing.
fn lookup_formation<'a>(
    setup: &'a MatchSetup,
    team: &TeamSetup,
    name: &str,
) -> Result<&'a FormationSpec, SetupError> {
    setup
        .formations
        .get(name)
        .ok_or_else(|| SetupError::FormationNotFound {
            team: team.name.clone(),
            formation: name.to_string(),
        })
}

/// Ensure a formation's slot count matches the roster size.
fn check_slot_count(team: &TeamSetup, name: &str, spec: &FormationSpec) -> Result<(), SetupError> {
    if team.players.len() != spec.slots.len() {
        return Err(SetupError::PlayerCountMismatch {
            team: team.name.clone(),
            formation: name.to_string(),
            players: team.players.len(),
            slots: spec.slots.len(),
        });
    }
    Ok(())
}

/// A slot's anchor in absolute field coordinates. A slot is an axial **hex**
/// `[q, r]` (§14 — players live on the grid); it resolves to that hex's center.
/// Team 0 as authored; team 1 is mirrored across x (negating the center's x is
/// exactly the mirror hex's center), so one template serves either side.
fn anchor_for(team_idx: usize, slot: &[i32; 2], hex_size: Fx) -> Vec2 {
    let center = Hex::new(slot[0], slot[1]).center(hex_size);
    if team_idx == 0 {
        center
    } else {
        Vec2::new(-center.x, center.y)
    }
}

impl MatchSetup {
    /// A ready-to-edit example matchup: two **phase shapes** (a high attacking
    /// push and a deep defending block, slot-aligned to the roles) fielded by
    /// two rosters. Authored in the team-0 frame: -x is each team's own scoring
    /// goal (where it offers), +x the goal it defends. This is what `tithe init`
    /// writes out and what the setup tests build against — a concrete stand-in
    /// for the game's tactics + roster screens.
    pub fn default_match() -> Self {
        use InPossessionRole as IP;
        use OutOfPossessionRole as OP;
        let mut formations = BTreeMap::new();
        // In-possession: slide toward -x (own goal) to offer — finisher parked
        // at the scoring spot, pressers up as support, anchor stepped up.
        // Slots are axial hexes [q, r] (§14).
        formations.insert(
            "high-push".to_string(),
            FormationSpec {
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
        );
        // Out-of-possession: drop toward +x (the defended goal) — anchor as the
        // last line, pressers forward to harry, finisher stays high to counter.
        formations.insert(
            "low-block".to_string(),
            FormationSpec {
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
        );
        Self {
            formations,
            teams: vec![
                // (name, attack_role, defend_role, [accuracy, range, handling,
                //  stripping, contesting, passing, positioning, pace, awareness])
                example_team(
                    "Embers",
                    &[
                        (
                            "Vale",
                            IP::Outlet,
                            OP::Sweeper,
                            [20, 20, 35, 70, 65, 45, 75, 40, 60],
                        ),
                        (
                            "Crane",
                            IP::Roamer,
                            OP::Warden,
                            [45, 45, 50, 50, 50, 55, 50, 70, 50],
                        ),
                        (
                            "Ash",
                            IP::Playmaker,
                            OP::Tracker,
                            [40, 35, 65, 35, 55, 80, 60, 50, 75],
                        ),
                        (
                            "Rook",
                            IP::BoxToBox,
                            OP::Warden,
                            [50, 45, 50, 55, 50, 50, 50, 55, 50],
                        ),
                        (
                            "Pyre",
                            IP::Roamer,
                            OP::Presser,
                            [50, 35, 40, 65, 60, 35, 45, 75, 45],
                        ),
                        (
                            "Sear",
                            IP::Finisher,
                            OP::Cheat,
                            [85, 30, 55, 30, 40, 50, 55, 60, 70],
                        ),
                        (
                            "Knell",
                            IP::Roamer,
                            OP::Destroyer,
                            [45, 40, 40, 60, 65, 40, 40, 65, 40],
                        ),
                    ],
                ),
                example_team(
                    "Wardens",
                    &[
                        (
                            "Holt",
                            IP::Outlet,
                            OP::Sweeper,
                            [25, 25, 35, 75, 60, 40, 80, 40, 65],
                        ),
                        (
                            "Bram",
                            IP::Roamer,
                            OP::Warden,
                            [50, 45, 50, 50, 50, 55, 45, 70, 50],
                        ),
                        (
                            "Fen",
                            IP::Playmaker,
                            OP::Tracker,
                            [45, 40, 70, 40, 50, 75, 60, 50, 80],
                        ),
                        (
                            "Cole",
                            IP::BoxToBox,
                            OP::Warden,
                            [50, 50, 50, 50, 55, 50, 50, 50, 50],
                        ),
                        (
                            "Dane",
                            IP::Roamer,
                            OP::Presser,
                            [50, 35, 40, 60, 65, 35, 45, 75, 45],
                        ),
                        (
                            "Gar",
                            IP::Finisher,
                            OP::Cheat,
                            [55, 85, 55, 35, 45, 55, 35, 45, 40],
                        ),
                        (
                            "Ward",
                            IP::Roamer,
                            OP::Destroyer,
                            [45, 40, 40, 65, 60, 45, 40, 65, 40],
                        ),
                    ],
                ),
            ],
        }
    }
}

/// Build an example team from compact `(name, attack_role, defend_role,
/// [accuracy, range, handling, stripping, contesting, passing, positioning,
/// pace, awareness])` tuples — keeps [`MatchSetup::default_match`] readable.
/// Both teams field the shared `high-push` / `low-block` phase shapes.
fn example_team(
    name: &str,
    players: &[(&str, InPossessionRole, OutOfPossessionRole, [u8; 9])],
) -> TeamSetup {
    TeamSetup {
        name: name.to_string(),
        attack_formation: "high-push".to_string(),
        defend_formation: "low-block".to_string(),
        players: players
            .iter()
            .map(
                |(
                    pname,
                    attack_role,
                    defend_role,
                    [accuracy, range, handling, stripping, contesting, passing, positioning, pace, awareness],
                )| {
                    PlayerSetup {
                        name: (*pname).to_string(),
                        attack_role: *attack_role,
                        defend_role: *defend_role,
                        accuracy: *accuracy,
                        range: *range,
                        handling: *handling,
                        stripping: *stripping,
                        contesting: *contesting,
                        passing: *passing,
                        positioning: *positioning,
                        pace: *pace,
                        awareness: *awareness,
                    }
                },
            )
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_match_builds_fourteen_agents() {
        let agents = build_agents(&MatchSetup::default_match()).expect("valid setup");
        assert_eq!(agents.len(), 14);
        assert_eq!(agents.iter().filter(|a| a.team == 0).count(), 7);
        // Ids are sequential, team 0 first.
        assert_eq!(agents[0].id, 0);
        assert_eq!(agents[7].team, 1);
    }

    #[test]
    fn authored_attributes_and_metadata_carry_through() {
        let agents = build_agents(&MatchSetup::default_match()).expect("valid setup");
        // Sear: accuracy 85 → 0.85, attack role Finisher, name preserved.
        let sear = agents.iter().find(|a| a.name == "Sear").expect("Sear");
        assert_eq!(sear.attack_role, InPossessionRole::Finisher);
        assert_eq!(
            sear.attributes.accuracy,
            Fx::from_num(85) / Fx::from_num(100)
        );
    }

    #[test]
    fn each_player_gets_both_phase_anchors_team_one_mirrored() {
        let agents = build_agents(&MatchSetup::default_match()).expect("valid setup");
        let size = SimConfig::default().hex_size;
        let c = |q, r| Hex::new(q, r).center(size);
        let mir = |v: Vec2| Vec2::new(-v.x, v.y);
        // Slot 0 (anchor): attack high-push hex [3,0], defend low-block hex [5,0].
        assert_eq!(agents[0].attack_anchor, c(3, 0));
        assert_eq!(agents[0].defend_anchor, c(5, 0));
        // Agents start on the defending anchor (faceoff soul is loose).
        assert_eq!(agents[0].anchor, c(5, 0));
        // Team 1's slot 0 mirrors both phases across x.
        assert_eq!(agents[7].attack_anchor, mir(c(3, 0)));
        assert_eq!(agents[7].defend_anchor, mir(c(5, 0)));
        assert_eq!(agents[7].anchor, mir(c(5, 0)));
    }

    #[test]
    fn apply_phase_switches_between_the_two_shapes() {
        let mut agents = build_agents(&MatchSetup::default_match()).expect("valid setup");
        let a = &mut agents[0];
        a.apply_phase(true);
        assert_eq!(a.anchor, a.attack_anchor);
        a.apply_phase(false);
        assert_eq!(a.anchor, a.defend_anchor);
    }

    #[test]
    fn player_count_must_match_slot_count() {
        let mut setup = MatchSetup::default_match();
        setup.teams[0].players.pop(); // 6 players, 7 slots
        match build_agents(&setup) {
            Err(SetupError::PlayerCountMismatch { players, slots, .. }) => {
                assert_eq!((players, slots), (6, 7));
            }
            other => panic!("expected PlayerCountMismatch, got {other:?}"),
        }
    }

    #[test]
    fn unknown_formation_is_rejected() {
        let mut setup = MatchSetup::default_match();
        setup.teams[1].defend_formation = "nonexistent".to_string();
        assert!(matches!(
            build_agents(&setup),
            Err(SetupError::FormationNotFound { .. })
        ));
    }

    #[test]
    fn wrong_team_count_is_rejected() {
        let mut setup = MatchSetup::default_match();
        setup.teams.pop();
        assert!(matches!(
            build_agents(&setup),
            Err(SetupError::WrongTeamCount(1))
        ));
    }

    #[test]
    fn out_of_range_attribute_is_rejected() {
        let mut setup = MatchSetup::default_match();
        setup.teams[0].players[0].accuracy = 150;
        assert!(matches!(
            build_agents(&setup),
            Err(SetupError::AttributeOutOfRange { value: 150, .. })
        ));
    }
}
