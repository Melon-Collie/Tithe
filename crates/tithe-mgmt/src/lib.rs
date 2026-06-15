//! # tithe-mgmt
//!
//! The **management layer**: the GM-coach loop that lives *around* a match.
//! Persistent players, the clubs that field them, a [`Season`] that schedules
//! them into a round-robin and tallies a table, and a [`Career`] that holds it
//! all — handing matchups to the sim and folding the results back in.
//!
//! ## The seam (load-bearing)
//!
//! This crate sits one level above [`tithe_sim`] and the dependency goes **one
//! way only** — the sim never knows the management layer exists (CLAUDE.md →
//! Committed architecture). Exactly two things cross the boundary:
//!
//! - **out to the sim:** a [`tithe_sim::MatchSetup`] built from a club's roster
//!   plus its [`Tactics`] (the two coach inputs — formations + role assignment);
//! - **back from the sim:** the match result — final score, winner, and the
//!   [`tithe_sim::BoxScore`] derived from the event stream (the per-player
//!   tallies that *deployment-based development* will later read).
//!
//! ## Same discipline as the sim, one layer up
//!
//! Like the sim, this crate is **pure, deterministic, and headless**: a career
//! is reproducible from its seed, all randomness is threaded through
//! [`tithe_sim::Rng`], and the crate does **no I/O** — it derives serde, but the
//! *consumers* (CLI, web) own reading and writing save files. That keeps the
//! save format a consumer concern and the career logic testable in isolation.
//!
//! ## A central player pool
//!
//! Players live in one pool on the [`Career`]; a [`Club`] holds only a roster of
//! [`PlayerId`]s into it. A player on no roster is a **free agent** — the pool is
//! what makes free agency, transfers, and drafts representable. (Their
//! *behaviour* — signing, wages, contracts — is a later money slice; this layer
//! just provides the shape.)
//!
//! ## Persistent vs. match-local identity
//!
//! A [`Player`] carries a stable [`PlayerId`] that lives for its whole career.
//! That is deliberately **distinct** from the match-local agent ids the sim
//! mints per match (`0..n`, where `id == index`; see `tithe-sim`). The
//! projection from roster to [`tithe_sim::MatchSetup`] is where one maps to the
//! other — keep them separate so benches, substitutions, and cross-match
//! history never confuse "this player" with "this match's slot 3".

pub mod career;
pub mod club;
pub mod player;
pub mod season;

pub use career::{Career, MatchRecord, MatchResult, NewPlayer};
pub use club::{Club, Tactics};
pub use player::{Player, PlayerId, Ratings};
pub use season::{Fixture, Schedule, Season, Standing};
