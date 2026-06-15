//! Money — kept deliberately minimal (design doc §7, §10): a **flat hard salary
//! cap with a floor**, player **contracts** (a wage + a term), and the single
//! economic lever, **unspent cap → development budget**. Money has exactly two
//! uses: players (wages) and player futures (development). No business sim, no
//! fiction-layer power.
//!
//! This module is the *structure and operations*. The AI GM judgement that drives
//! it — valuing players through fog, weighing win-now vs. develop-later — is
//! deliberately **not** here yet; it will act through this API later.

use serde::{Deserialize, Serialize};

/// Default league dials (abstract money units) — an initial tuning guess, not
/// commitments. A balanced seven-a-side roster of average players sits near the
/// cap; cheaper rosters open up development budget.
pub const DEFAULT_SALARY_CAP: u32 = 1200;
pub const DEFAULT_SALARY_FLOOR: u32 = 700;

/// Money paid per point of overall rating — the market wage scale.
const WAGE_PER_RATING: u32 = 3;

/// The largest growth bonus (percent) a club's development budget can buy, at a
/// full unspent cap. Scales linearly with `budget / cap`.
pub(crate) const MAX_DEVELOPMENT_BOOST: u32 = 50;

/// A market wage for a player of this overall rating — what a club pays to roster
/// him. Linear in overall for now (tunable).
pub fn wage_for(overall: u8) -> u32 {
    overall as u32 * WAGE_PER_RATING
}

/// A player's deal with his club: his wage and how many seasons remain on it. A
/// free agent has no contract. The term ticks down each season but does **not**
/// auto-release at zero (renewal/expiry is an AI decision, deferred) — a
/// `years_remaining` of 0 just flags an expiring deal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub wage: u32,
    pub years_remaining: u8,
}

impl Contract {
    /// A new deal at the given wage and term.
    pub fn new(wage: u32, years: u8) -> Self {
        Contract {
            wage,
            years_remaining: years,
        }
    }
}

/// Why a roster move was rejected. These are decision/authoring errors, surfaced
/// with enough context to explain the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceError {
    /// The signing would push the club's payroll past the hard cap.
    OverCap { payroll: u32, cap: u32 },
    /// The club already fields a full roster — release someone first.
    RosterFull,
    /// The player is already on a club's roster (not a free agent).
    NotAFreeAgent,
    /// The player isn't on the club whose roster the move targets.
    NotOnRoster,
}

impl std::fmt::Display for FinanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinanceError::OverCap { payroll, cap } => {
                write!(
                    f,
                    "signing would put payroll at {payroll}, over the cap of {cap}"
                )
            }
            FinanceError::RosterFull => write!(f, "roster is full — release a player first"),
            FinanceError::NotAFreeAgent => write!(f, "player is already under contract"),
            FinanceError::NotOnRoster => write!(f, "player is not on that club's roster"),
        }
    }
}

impl std::error::Error for FinanceError {}
