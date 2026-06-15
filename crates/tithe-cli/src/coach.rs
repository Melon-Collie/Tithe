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

use std::collections::BTreeMap;
use tithe_sim::setup::{FormationSpec, TeamSetup};
use tithe_sim::{
    Attribute, InPossessionRole as IP, MatchSetup, OutOfPossessionRole as OP, PlayerSetup, Rng,
    Simulation,
};

/// A player the GM hands the coach. `attrs` is indexed by [`Attribute`] in its
/// canonical order (`attrs[Attribute::Handling as usize]`); the sim owns that
/// ordering, so this tool can't drift from it.
struct Player {
    name: String,
    attrs: [u8; 9],
}

/// One positional slot: the role-pair (what it wants) plus its phase anchors as
/// axial hexes `[q, r]` in the team-0 frame — `attack` pushed toward the offering
/// end (-x), `defend` dropped toward the defended end (+x). The sim mirrors them
/// for team 1.
struct Slot {
    label: &'static str,
    attack: IP,
    defend: OP,
    attack_anchor: [i32; 2],
    defend_anchor: [i32; 2],
}

/// A formation: a name and seven positional slots.
struct Template {
    name: &'static str,
    slots: [Slot; 7],
}

#[allow(clippy::too_many_arguments)]
fn s(label: &'static str, attack: IP, defend: OP, ax: i32, ar: i32, dx: i32, dr: i32) -> Slot {
    Slot {
        label,
        attack,
        defend,
        attack_anchor: [ax, ar],
        defend_anchor: [dx, dr],
    }
}

fn templates() -> Vec<Template> {
    use IP::*;
    use OP::*;
    vec![
        // 2-3-1: keeper, two central defenders, a wide-three midfield, lone striker.
        Template {
            name: "2-3-1",
            slots: [
                //   label    attack    defend     atk[q,r]  def[q,r]
                s("Goalie", Outlet, Sweeper, 3, 0, 5, 0),
                s("Def-L", Outlet, Marker, 2, -1, 4, -1),
                s("Def-R", Outlet, Marker, 1, 1, 3, 1),
                s("Mid-L", Runner, Hawk, -2, -2, 1, -2),
                s("Mid-C", Playmaker, Marker, -1, 0, 2, 0),
                s("Mid-R", Runner, Hawk, -3, 2, 0, 2),
                s("Fwd", Finisher, Cheat, -5, 0, -1, 0),
            ],
        },
        // 3-2-1: keeper, back three (middle is a Destroyer), a two-midfield, striker.
        Template {
            name: "3-2-1",
            slots: [
                s("Goalie", Outlet, Sweeper, 3, 0, 5, 0),
                s("Def-L", Outlet, Marker, 2, -2, 4, -2),
                s("Def-C", Runner, Destroyer, 2, 0, 4, 0),
                s("Def-R", Outlet, Marker, 0, 2, 2, 2),
                s("Mid-L", Pivot, Hawk, -2, -1, 1, -1),
                s("Mid-R", Playmaker, Hawk, -3, 1, 0, 1),
                s("Fwd", Finisher, Cheat, -5, 0, -1, 0),
            ],
        },
    ]
}

/// Build a want-vector from named weights (any attribute left out is 0).
/// Keying by [`Attribute`] instead of writing bare positional literals means a
/// reorder of the attribute set can't silently misalign a role's priorities.
fn want(pairs: &[(Attribute, f64)]) -> [f64; 9] {
    let mut w = [0.0; 9];
    for &(a, v) in pairs {
        w[a as usize] = v;
    }
    w
}

/// What an in-possession role leans on (weights over the attributes).
fn attack_want(r: IP) -> [f64; 9] {
    use Attribute::*;
    match r {
        // Runner really wants Pace — he drives at the space he opens.
        IP::Runner => want(&[(Pace, 3.), (Handling, 2.), (Contesting, 1.)]),
        IP::Outlet => want(&[(Passing, 3.), (Handling, 1.), (Positioning, 2.)]),
        IP::Pivot => want(&[(Passing, 3.), (Awareness, 2.), (Positioning, 1.)]),
        IP::Playmaker => want(&[(Passing, 2.), (Handling, 2.), (Awareness, 2.)]),
        IP::Finisher => want(&[(Accuracy, 3.), (Range, 2.), (Pace, 1.)]),
    }
}

/// What an out-of-possession role leans on.
fn defend_want(r: OP) -> [f64; 9] {
    use Attribute::*;
    match r {
        OP::Sweeper => want(&[(Handling, 1.), (Stripping, 1.), (Positioning, 3.)]),
        OP::Marker => want(&[(Positioning, 2.), (Awareness, 2.), (Contesting, 2.)]),
        OP::Destroyer => want(&[(Stripping, 3.), (Pace, 2.)]),
        OP::Hawk => want(&[
            (Contesting, 2.),
            (Awareness, 2.),
            (Positioning, 1.),
            (Pace, 1.),
        ]),
        OP::Cheat => want(&[(Accuracy, 2.), (Range, 1.), (Pace, 2.)]),
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
    let mut ranked = Attribute::ALL;
    ranked.sort_by_key(|&a| std::cmp::Reverse(attrs[a as usize]));
    let label = |a: Attribute| format!("{} {}", a.short(), attrs[a as usize]);
    format!("{}, {}", label(ranked[0]), label(ranked[1]))
}

/// Generate a varied squad of 7 (the GM's roster) — random, with one or two
/// attributes spiked so each player has an identity the coach can read.
fn generate_squad(seed: u64, prefix: char) -> Vec<Player> {
    let mut rng = Rng::new(seed);
    let mut players = Vec::new();
    for i in 0..7 {
        let mut attrs = [0u8; 9];
        for a in attrs.iter_mut() {
            *a = 30 + rng.below(40) as u8; // 30..70 baseline
        }
        for _ in 0..1 + rng.below(2) {
            attrs[rng.below(9) as usize] = 80 + rng.below(20) as u8; // a spike or two
        }
        players.push(Player {
            name: format!("{prefix}{i}"),
            attrs,
        });
    }
    players
}

/// Turn a coached lineup into authored inputs: a [`TeamSetup`] plus its two phase
/// [`FormationSpec`]s (keyed `key_attack` / `key_defend`). Player at slot *i* gets
/// that slot's roles and anchors — exactly the two coach inputs a human authors.
fn build_team(
    team_name: &str,
    key: &str,
    t: &Template,
    assign: &[usize],
    players: &[Player],
) -> (TeamSetup, (String, FormationSpec), (String, FormationSpec)) {
    let attack = FormationSpec {
        slots: t.slots.iter().map(|s| s.attack_anchor).collect(),
    };
    let defend = FormationSpec {
        slots: t.slots.iter().map(|s| s.defend_anchor).collect(),
    };
    let roster = (0..t.slots.len())
        .map(|slot| {
            let a = players[assign[slot]].attrs;
            let g = |attr: Attribute| a[attr as usize];
            PlayerSetup {
                name: players[assign[slot]].name.clone(),
                attack_role: t.slots[slot].attack,
                defend_role: t.slots[slot].defend,
                accuracy: g(Attribute::Accuracy),
                range: g(Attribute::Range),
                handling: g(Attribute::Handling),
                stripping: g(Attribute::Stripping),
                contesting: g(Attribute::Contesting),
                passing: g(Attribute::Passing),
                positioning: g(Attribute::Positioning),
                pace: g(Attribute::Pace),
                awareness: g(Attribute::Awareness),
            }
        })
        .collect();
    let team = TeamSetup {
        name: team_name.to_string(),
        attack_formation: format!("{key}_attack"),
        defend_formation: format!("{key}_defend"),
        players: roster,
    };
    (
        team,
        (format!("{key}_attack"), attack),
        (format!("{key}_defend"), defend),
    )
}

/// Print a coached squad's chosen formation and lineup.
fn report(squad_name: &str, players: &[Player], templates: &[Template]) -> (usize, Vec<usize>) {
    let (chosen, total, assign) = coach(players, templates);
    let idx = templates
        .iter()
        .position(|t| std::ptr::eq(t, chosen))
        .unwrap();
    println!("\n== {squad_name}: {} (fit {total:.0}) ==", chosen.name);
    for t in templates {
        let wants: Vec<[f64; 9]> = t.slots.iter().map(slot_want).collect();
        let (tf, _) = best_assignment(&wants, players);
        let mark = if std::ptr::eq(t, chosen) { " <-" } else { "" };
        println!("    {:<6} fit {:>6.0}{}", t.name, tf, mark);
    }
    println!("  pos     role (attack/defend)       fit  player");
    for (slot, &p) in assign.iter().enumerate() {
        let sl = &chosen.slots[slot];
        let f = fit(&slot_want(sl), &players[p].attrs);
        let roles = format!("{:?}/{:?}", sl.attack, sl.defend);
        println!(
            "  {:<7} {roles:<24} {f:>5.0}  {} ({})",
            sl.label,
            players[p].name,
            standouts(&players[p].attrs)
        );
    }
    (idx, assign)
}

pub fn run(args: &[String]) {
    let seed = crate::flag_or(args, "--seed", 1u64);
    let matches = crate::flag_or(args, "--matches", 40u64);
    let templates = templates();

    // Two GM rosters; the coach lineups each, then they play.
    let squad_a = generate_squad(seed, 'A');
    let squad_b = generate_squad(seed.wrapping_add(777), 'B');
    let (ia, assign_a) = report("Squad A", &squad_a, &templates);
    let (ib, assign_b) = report("Squad B", &squad_b, &templates);

    let (team_a, aa, ad) = build_team("Aces", "a", &templates[ia], &assign_a, &squad_a);
    let (team_b, ba, bd) = build_team("Bolts", "b", &templates[ib], &assign_b, &squad_b);
    let mut formations = BTreeMap::new();
    for (k, f) in [aa, ad, ba, bd] {
        formations.insert(k, f);
    }
    let setup = MatchSetup {
        formations,
        teams: vec![team_a, team_b],
    };

    let mut wins = [0u32, 0u32];
    for s in 0..matches {
        let mut sim = Simulation::from_setup(&setup, s).expect("coached setup is valid");
        let mut ticks = 0;
        while sim.winner().is_none() && ticks < 200_000 {
            sim.tick();
            ticks += 1;
        }
        if let Some(w) = sim.winner() {
            wins[w as usize] += 1;
        }
    }
    println!(
        "\ncoached match — Aces ({}) {} - {} Bolts ({}), {matches} games",
        templates[ia].name, wins[0], wins[1], templates[ib].name
    );
}
