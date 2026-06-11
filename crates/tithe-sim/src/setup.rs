//! The coach-input boundary: a serializable description of a matchup that the
//! sim turns into a starting roster.
//!
//! This is the concrete shape of §9's "serialized inputs go in." A
//! [`MatchSetup`] carries the **two coach inputs** plus the personnel they act
//! on (§7 — teams differ only by personnel):
//!
//! - a **library of named formations** (positioning templates) — pick one per
//!   team, so two teams can field different shapes;
//! - per team, a **roster** of players, each with a [`Role`] (the casting) and
//!   the four attributes;
//! - the **assignment** is positional: the *n*-th player fills the *n*-th slot
//!   of the chosen formation.
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
use crate::world::{Agent, Attributes, Role};
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
    /// One `[x, y]` anchor per slot. Integer units (the arena spans ±50 × ±30).
    pub slots: Vec<[i32; 2]>,
}

/// One team: a display name, the formation it fields (by key into
/// [`MatchSetup::formations`]), and its players in slot order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSetup {
    pub name: String,
    pub formation: String,
    pub players: Vec<PlayerSetup>,
}

/// One authored player. Attributes are integer percentiles `0..=100`
/// (`finishing` 80 = the Fx `0.80` the sim consumes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSetup {
    pub name: String,
    #[serde(default)]
    pub role: Role,
    pub finishing: u8,
    pub stripping: u8,
    pub contesting: u8,
    pub passing: u8,
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
            finishing: one("finishing", self.finishing)?,
            stripping: one("stripping", self.stripping)?,
            contesting: one("contesting", self.contesting)?,
            passing: one("passing", self.passing)?,
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
/// sequential). Team 1's formation is mirrored across x. Errors point at the
/// authoring mistake (see [`SetupError`]); no RNG is consumed — attributes are
/// authored, so a match built from a setup is reproducible from the seed alone.
pub fn build_agents(setup: &MatchSetup) -> Result<Vec<Agent>, SetupError> {
    if setup.teams.len() != 2 {
        return Err(SetupError::WrongTeamCount(setup.teams.len()));
    }
    let mut agents = Vec::new();
    for (team_idx, team) in setup.teams.iter().enumerate() {
        let spec =
            setup
                .formations
                .get(&team.formation)
                .ok_or_else(|| SetupError::FormationNotFound {
                    team: team.name.clone(),
                    formation: team.formation.clone(),
                })?;
        if team.players.len() != spec.slots.len() {
            return Err(SetupError::PlayerCountMismatch {
                team: team.name.clone(),
                formation: team.formation.clone(),
                players: team.players.len(),
                slots: spec.slots.len(),
            });
        }
        for (slot, player) in spec.slots.iter().zip(&team.players) {
            let anchor = anchor_for(team_idx, slot);
            let attributes = player.to_attributes()?;
            let id = agents.len() as u32;
            agents.push(Agent {
                id,
                name: player.name.clone(),
                team: team_idx as u8,
                role: player.role,
                pos: anchor,
                target: anchor,
                anchor,
                stagger: 0,
                stamina: Fx::from_num(1),
                attributes,
            });
        }
    }
    Ok(agents)
}

/// A slot's anchor in absolute field coordinates: team 0 as authored, team 1
/// mirrored across x (so a single template serves either side).
fn anchor_for(team_idx: usize, slot: &[i32; 2]) -> Vec2 {
    let x = Fx::from_num(slot[0]);
    let y = Fx::from_num(slot[1]);
    if team_idx == 0 {
        Vec2::new(x, y)
    } else {
        Vec2::new(-x, y)
    }
}

impl MatchSetup {
    /// A ready-to-edit example matchup: the default seven-slot "spine" shape,
    /// fielded by two distinct rosters. This is what `tithe init` writes out and
    /// what the setup tests build against — a concrete, authored stand-in for the
    /// game's roster screen.
    pub fn default_match() -> Self {
        let mut formations = BTreeMap::new();
        formations.insert(
            "spine".to_string(),
            FormationSpec {
                slots: vec![
                    [-30, 0],  // deep safety (own goal)
                    [-5, -15], // midfield
                    [-5, 15],
                    [10, 0], // spine — contests the center soul
                    [28, -16],
                    [28, 0], // forward press (the opponent's goal)
                    [28, 16],
                ],
            },
        );
        Self {
            formations,
            teams: vec![
                example_team(
                    "Embers",
                    &[
                        ("Vale", Role::Anchor, [20, 70, 65, 45]),
                        ("Crane", Role::Rover, [45, 50, 50, 55]),
                        ("Ash", Role::Playmaker, [40, 35, 55, 80]),
                        ("Rook", Role::Rover, [50, 55, 50, 50]),
                        ("Pyre", Role::Presser, [55, 65, 60, 35]),
                        ("Sear", Role::Finisher, [85, 30, 40, 50]),
                        ("Knell", Role::Presser, [50, 60, 65, 40]),
                    ],
                ),
                example_team(
                    "Wardens",
                    &[
                        ("Holt", Role::Anchor, [25, 75, 60, 40]),
                        ("Bram", Role::Rover, [50, 50, 50, 55]),
                        ("Fen", Role::Playmaker, [45, 40, 50, 75]),
                        ("Cole", Role::Rover, [50, 50, 55, 50]),
                        ("Dane", Role::Presser, [55, 60, 65, 35]),
                        ("Gar", Role::Finisher, [80, 35, 45, 55]),
                        ("Ward", Role::Presser, [45, 65, 60, 45]),
                    ],
                ),
            ],
        }
    }
}

/// Build an example team from compact `(name, role, [fin, strip, cont, pass])`
/// tuples — keeps [`MatchSetup::default_match`] readable.
fn example_team(name: &str, players: &[(&str, Role, [u8; 4])]) -> TeamSetup {
    TeamSetup {
        name: name.to_string(),
        formation: "spine".to_string(),
        players: players
            .iter()
            .map(
                |(pname, role, [finishing, stripping, contesting, passing])| PlayerSetup {
                    name: (*pname).to_string(),
                    role: *role,
                    finishing: *finishing,
                    stripping: *stripping,
                    contesting: *contesting,
                    passing: *passing,
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
        // Sear: finishing 85 → 0.85, role Finisher, name preserved.
        let sear = agents.iter().find(|a| a.name == "Sear").expect("Sear");
        assert_eq!(sear.role, Role::Finisher);
        assert_eq!(
            sear.attributes.finishing,
            Fx::from_num(85) / Fx::from_num(100)
        );
    }

    #[test]
    fn team_one_formation_is_mirrored_across_x() {
        let agents = build_agents(&MatchSetup::default_match()).expect("valid setup");
        // Slot 0 is the deep safety at x=-30; team 1's mirrors to +30.
        assert_eq!(
            agents[0].anchor,
            Vec2::new(Fx::from_num(-30), Fx::from_num(0))
        );
        assert_eq!(
            agents[7].anchor,
            Vec2::new(Fx::from_num(30), Fx::from_num(0))
        );
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
        setup.teams[1].formation = "nonexistent".to_string();
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
        setup.teams[0].players[0].finishing = 150;
        assert!(matches!(
            build_agents(&setup),
            Err(SetupError::AttributeOutOfRange { value: 150, .. })
        ));
    }
}
