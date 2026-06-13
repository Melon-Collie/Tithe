//! Per-player box score — a pure derivation over the event stream.
//!
//! The simulation never tracks stats; the event stream is its first-class output
//! (CLAUDE.md → Committed architecture), and a box score *falls out of it*. This
//! is a consumer of that stream, not state inside `Simulation` — so it stays a
//! deterministic, testable pure function, and the same derivation serves the CLI
//! today and the web viewer / season aggregation later.
//!
//! It is also the measuring instrument for *validation*: per-player attribution
//! is how we confirm an attribute or role does what it's designed to (does high
//! Stripping win more strips? does high Accuracy convert more offerings?).

use crate::event::Event;

/// One player's tallies for a match. Indexed by agent id in [`BoxScore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerLine {
    /// Offerings that scored (banked souls credited to the carrier).
    pub goals: u32,
    /// Offerings taken (the shot attempts) — `goals / offerings` is the convert %.
    pub offerings: u32,
    /// Passes attempted.
    pub passes: u32,
    /// Passes that reached a teammate (attempted minus intercepted).
    pub passes_completed: u32,
    /// Enemy passes this player picked off.
    pub interceptions: u32,
    /// Strips this player won as the defender (clean takeaways).
    pub strips_won: u32,
    /// Strips this player committed to (won + whiffed) — `won / attempted` is the
    /// success rate.
    pub strips_attempted: u32,
    /// Times this player was stripped while carrying (lost the soul).
    pub strips_suffered: u32,
    /// Loose souls claimed (faceoff draws and rebound recoveries).
    pub recoveries: u32,
}

/// A match's per-player tallies, indexed by agent id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxScore {
    pub players: Vec<PlayerLine>,
}

impl BoxScore {
    /// Derive the box score from a match's full event stream. `num_agents` sizes
    /// the table (one line per agent); events naming an out-of-range agent are
    /// ignored rather than panicking.
    pub fn from_events(events: &[Event], num_agents: usize) -> Self {
        let mut players = vec![PlayerLine::default(); num_agents];
        // Exactly one pass is ever in flight, so an interception always belongs to
        // the most recent `PassMade`. We count each pass completed optimistically
        // and undo it if the next event is its interception.
        let mut last_pass_from: Option<u32> = None;

        for event in events {
            match *event {
                Event::OfferingResolved {
                    carrier, scored, ..
                } => {
                    if let Some(p) = players.get_mut(carrier as usize) {
                        p.offerings += 1;
                        if scored {
                            p.goals += 1;
                        }
                    }
                }
                Event::PassMade { from, .. } => {
                    if let Some(p) = players.get_mut(from as usize) {
                        p.passes += 1;
                        p.passes_completed += 1; // optimistic; undone on interception
                    }
                    last_pass_from = Some(from);
                }
                Event::PassIntercepted { by } => {
                    if let Some(p) = players.get_mut(by as usize) {
                        p.interceptions += 1;
                    }
                    if let Some(from) = last_pass_from.take() {
                        if let Some(p) = players.get_mut(from as usize) {
                            p.passes_completed = p.passes_completed.saturating_sub(1);
                        }
                    }
                }
                Event::StripAttempt {
                    defender,
                    carrier,
                    success,
                    ..
                } => {
                    if let Some(p) = players.get_mut(defender as usize) {
                        p.strips_attempted += 1;
                        if success {
                            p.strips_won += 1;
                        }
                    }
                    if success {
                        if let Some(p) = players.get_mut(carrier as usize) {
                            p.strips_suffered += 1;
                        }
                    }
                }
                Event::SoulClaimed { agent } => {
                    if let Some(p) = players.get_mut(agent as usize) {
                        p.recoveries += 1;
                    }
                }
                _ => {}
            }
        }
        BoxScore { players }
    }

    /// Add another box score into this one, line by line (season/batch totals).
    /// Both must be the same size.
    pub fn add(&mut self, other: &BoxScore) {
        for (a, b) in self.players.iter_mut().zip(&other.players) {
            a.goals += b.goals;
            a.offerings += b.offerings;
            a.passes += b.passes;
            a.passes_completed += b.passes_completed;
            a.interceptions += b.interceptions;
            a.strips_won += b.strips_won;
            a.strips_attempted += b.strips_attempted;
            a.strips_suffered += b.strips_suffered;
            a.recoveries += b.recoveries;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tallies_goals_passes_strips_from_the_stream() {
        let events = vec![
            Event::SoulClaimed { agent: 0 },
            Event::PassMade {
                from: 0,
                to: 1,
                chance: 80,
            },
            // A second pass that gets picked off.
            Event::PassMade {
                from: 1,
                to: 2,
                chance: 50,
            },
            Event::PassIntercepted { by: 9 },
            Event::StripAttempt {
                defender: 9,
                carrier: 3,
                chance: 60,
                success: true,
            },
            Event::StripAttempt {
                defender: 4,
                carrier: 9,
                chance: 40,
                success: false,
            },
            Event::OfferingResolved {
                carrier: 5,
                chance: 70,
                scored: true,
            },
            Event::OfferingResolved {
                carrier: 5,
                chance: 30,
                scored: false,
            },
        ];
        let bx = BoxScore::from_events(&events, 10);

        // Agent 0: one recovery, one completed pass.
        assert_eq!(bx.players[0].recoveries, 1);
        assert_eq!(bx.players[0].passes, 1);
        assert_eq!(bx.players[0].passes_completed, 1);
        // Agent 1: one pass attempted, intercepted → not completed.
        assert_eq!(bx.players[1].passes, 1);
        assert_eq!(bx.players[1].passes_completed, 0);
        // Agent 9: an interception, a won strip, and was the victim of a whiffed
        // strip (no suffered count — only successful strips are suffered).
        assert_eq!(bx.players[9].interceptions, 1);
        assert_eq!(bx.players[9].strips_won, 1);
        assert_eq!(bx.players[9].strips_attempted, 1);
        assert_eq!(bx.players[9].strips_suffered, 0);
        // Agent 3: stripped once.
        assert_eq!(bx.players[3].strips_suffered, 1);
        // Agent 4: a whiffed strip — attempted but not won.
        assert_eq!(bx.players[4].strips_attempted, 1);
        assert_eq!(bx.players[4].strips_won, 0);
        // Agent 5: two offerings, one scored.
        assert_eq!(bx.players[5].offerings, 2);
        assert_eq!(bx.players[5].goals, 1);
    }

    #[test]
    fn add_accumulates_line_by_line() {
        let a_events = vec![Event::OfferingResolved {
            carrier: 0,
            chance: 50,
            scored: true,
        }];
        let mut total = BoxScore::from_events(&a_events, 2);
        let more = BoxScore::from_events(&a_events, 2);
        total.add(&more);
        assert_eq!(total.players[0].goals, 2);
        assert_eq!(total.players[0].offerings, 2);
    }
}
