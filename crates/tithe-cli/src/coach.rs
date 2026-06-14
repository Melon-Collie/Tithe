//! `coach`: a hand-authored AI coach — fit a squad to a tactical template.
//!
//! The management split (CLAUDE.md → the two coach inputs): a **GM** decides
//! *which players* you have; a **coach** turns that squad into the two coach
//! inputs — a formation and a role assignment. This is a v1 coach: a small
//! library of tactical templates (each a formation + a role-pair per slot), and
//! for each it solves the best player→slot assignment by *attribute fit*, then
//! fields the template its squad fits best. Reasonable, not optimal-by-search —
//! the role "wants" are hand-authored from what each role leans on.
//!
//! It's a *consumer*: it emits the same authored inputs a human would, never
//! touching the sim. Floats are fine here (analysis/tooling boundary).

use tithe_sim::{InPossessionRole as IP, OutOfPossessionRole as OP, Rng};

/// Attribute order used throughout: acc, rng, han, str, con, pas, pos, pace, awr.
const ATTR_NAMES: [&str; 9] = [
    "Acc", "Rng", "Han", "Str", "Con", "Pas", "Pos", "Pace", "Awr",
];

/// A player the GM hands the coach.
struct Player {
    name: String,
    attrs: [u8; 9],
}

/// One slot of a template: the phase anchors (shared across templates in v1) and
/// the role-pair that defines what the slot wants.
struct Slot {
    attack: IP,
    defend: OP,
}

/// A tactical identity: a name and seven role-pairs. Anchors are shared (the
/// default high-push / low-block shape) in v1; templates differ by role mix, so
/// different squads prefer different ones.
struct Template {
    name: &'static str,
    slots: [Slot; 7],
}

fn s(attack: IP, defend: OP) -> Slot {
    Slot { attack, defend }
}

fn templates() -> Vec<Template> {
    vec![
        Template {
            name: "Balanced",
            slots: [
                s(IP::Outlet, OP::Sweeper),
                s(IP::Roamer, OP::Warden),
                s(IP::Playmaker, OP::Tracker),
                s(IP::BoxToBox, OP::Warden),
                s(IP::Roamer, OP::Presser),
                s(IP::Finisher, OP::Cheat),
                s(IP::Roamer, OP::Destroyer),
            ],
        },
        Template {
            name: "High press",
            slots: [
                s(IP::Roamer, OP::Presser),
                s(IP::Roamer, OP::Presser),
                s(IP::BoxToBox, OP::Destroyer),
                s(IP::Playmaker, OP::Tracker),
                s(IP::Finisher, OP::Cheat),
                s(IP::Outlet, OP::Sweeper),
                s(IP::Roamer, OP::Destroyer),
            ],
        },
        Template {
            name: "Low block",
            slots: [
                s(IP::Outlet, OP::Sweeper),
                s(IP::Playmaker, OP::Warden),
                s(IP::Playmaker, OP::Tracker),
                s(IP::BoxToBox, OP::Warden),
                s(IP::Roamer, OP::Sweeper),
                s(IP::Finisher, OP::Cheat),
                s(IP::Outlet, OP::Tracker),
            ],
        },
    ]
}

/// What an in-possession role leans on (weights over the attribute order).
fn attack_want(r: IP) -> [f64; 9] {
    //               acc rng han str con pas pos pace awr
    match r {
        IP::BoxToBox => [0., 0., 2., 0., 1., 1., 0., 2., 0.],
        IP::Roamer => [0., 0., 2., 1., 0., 0., 0., 2., 0.],
        IP::Playmaker => [0., 0., 0., 0., 0., 3., 1., 0., 2.],
        IP::Outlet => [0., 0., 2., 0., 0., 2., 1., 0., 0.],
        IP::Finisher => [3., 2., 0., 0., 0., 0., 0., 1., 0.],
    }
}

/// What an out-of-possession role leans on.
fn defend_want(r: OP) -> [f64; 9] {
    //               acc rng han str con pas pos pace awr
    match r {
        OP::Destroyer => [0., 0., 0., 3., 0., 0., 0., 2., 0.],
        OP::Presser => [0., 0., 0., 2., 2., 0., 0., 2., 0.],
        OP::Warden => [0., 0., 1., 0., 2., 0., 2., 0., 0.],
        OP::Sweeper => [0., 0., 1., 1., 0., 0., 3., 0., 0.],
        OP::Cheat => [2., 1., 0., 0., 0., 0., 0., 2., 0.],
        OP::Tracker => [0., 0., 0., 0., 2., 0., 2., 0., 1.],
    }
}

/// The combined attribute want of a slot (its two roles).
fn slot_want(slot: &Slot) -> [f64; 9] {
    let a = attack_want(slot.attack);
    let d = defend_want(slot.defend);
    std::array::from_fn(|k| a[k] + d[k])
}

/// How well a player suits a want (dot product of want · attributes).
fn fit(want: &[f64; 9], attrs: &[u8; 9]) -> f64 {
    (0..9).map(|k| want[k] * attrs[k] as f64).sum()
}

/// Best assignment of players to slots, maximizing total fit. Brute-forces all
/// 7! orderings (5040 — trivial) for an exact, deterministic answer.
/// `wants[slot]` is the slot's want; returns `(total_fit, assignment)` where
/// `assignment[slot]` is the chosen player index.
fn best_assignment(wants: &[[f64; 9]], players: &[Player]) -> (f64, Vec<usize>) {
    let n = wants.len();
    let mut used = vec![false; n];
    let mut cur = vec![0usize; n];
    let mut best = (f64::MIN, vec![0usize; n]);
    fn go(
        slot: usize,
        acc: f64,
        wants: &[[f64; 9]],
        players: &[Player],
        used: &mut [bool],
        cur: &mut [usize],
        best: &mut (f64, Vec<usize>),
    ) {
        if slot == wants.len() {
            if acc > best.0 {
                *best = (acc, cur.to_vec());
            }
            return;
        }
        for p in 0..players.len() {
            if used[p] {
                continue;
            }
            used[p] = true;
            cur[slot] = p;
            let next = acc + fit(&wants[slot], &players[p].attrs);
            go(slot + 1, next, wants, players, used, cur, best);
            used[p] = false;
        }
    }
    go(0, 0.0, wants, players, &mut used, &mut cur, &mut best);
    best
}

/// Pick the template the squad fits best, and its assignment.
fn coach<'a>(players: &[Player], templates: &'a [Template]) -> (&'a Template, f64, Vec<usize>) {
    let mut best: Option<(&Template, f64, Vec<usize>)> = None;
    for t in templates {
        let wants: Vec<[f64; 9]> = t.slots.iter().map(slot_want).collect();
        let (total, assign) = best_assignment(&wants, players);
        if best.as_ref().is_none_or(|b| total > b.1) {
            best = Some((t, total, assign));
        }
    }
    best.expect("at least one template")
}

/// A player's two standout attributes, for legible output.
fn standouts(attrs: &[u8; 9]) -> String {
    let mut idx: Vec<usize> = (0..9).collect();
    idx.sort_by_key(|&k| std::cmp::Reverse(attrs[k]));
    format!(
        "{} {}, {} {}",
        ATTR_NAMES[idx[0]], attrs[idx[0]], ATTR_NAMES[idx[1]], attrs[idx[1]]
    )
}

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);

    // Generate a varied squad of 7 (the GM's roster) — each player random, with
    // one or two attributes spiked so the coach's choices are legible.
    let mut rng = Rng::new(seed);
    let mut players = Vec::new();
    for i in 0..7 {
        let mut attrs = [0u8; 9];
        for a in attrs.iter_mut() {
            *a = 30 + rng.below(40) as u8; // 30..70 baseline
        }
        // Spike one or two attributes so this player has an identity.
        let spikes = 1 + rng.below(2);
        for _ in 0..spikes {
            let k = rng.below(9) as usize;
            attrs[k] = 80 + rng.below(20) as u8; // 80..100
        }
        players.push(Player {
            name: format!("P{i}"),
            attrs,
        });
    }

    let templates = templates();
    let (chosen, total, assign) = coach(&players, &templates);

    println!("== Coach lineup (seed {seed}) ==");
    println!("squad:");
    for p in &players {
        println!("  {:<4} {}", p.name, standouts(&p.attrs));
    }
    // Show why this template — its fit vs the others.
    println!("\ntemplate fits:");
    for t in &templates {
        let wants: Vec<[f64; 9]> = t.slots.iter().map(slot_want).collect();
        let (tf, _) = best_assignment(&wants, &players);
        let mark = if std::ptr::eq(t, chosen) { " <- chosen" } else { "" };
        println!("  {:<12} {:>7.0}{}", t.name, tf, mark);
    }

    println!("\nlineup ({}, total fit {total:.0}):", chosen.name);
    println!("  slot role (attack/defend)     fit  player");
    for (slot, &p) in assign.iter().enumerate() {
        let sl = &chosen.slots[slot];
        let f = fit(&slot_want(sl), &players[p].attrs);
        let roles = format!("{:?}/{:?}", sl.attack, sl.defend);
        println!(
            "  {slot:<4} {roles:<22} {f:>5.0}  {} ({})",
            players[p].name,
            standouts(&players[p].attrs)
        );
    }
}
