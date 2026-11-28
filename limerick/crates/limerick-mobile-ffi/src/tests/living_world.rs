//! The five living-world proofs (product spec Milestone 6), through the C
//! boundary against the canonical world: memory, gossip, task, weather
//! behaviour and scheduled movement. `docs/test-plans/phase-6-test-cases.md`
//! numbers the cases; each test names its case.
//!
//! Inference is scripted at the Endpoint boundary: the test answers the
//! pending `rundale-intent` and `rundale-dialogue` calls, as the Swift host
//! does, so no model runs and every reply is fixed. The authored facts are
//! in `mods/rundale/living-world.json` and `world-sheet.txt`.
//!
//! A case whose proof issue has not landed is in a `pending` module behind
//! the `living-world-pending` feature, so it fails only when
//! `just mobile-verify` builds it (`--phase 6`, `--phase all` or `--smoke`),
//! never in the default `cargo test`.

use super::*;

const CATTLE_BUYER: &str = "cattle buyer";
const PEIG: &str = "Peig Hannigan";
const MICHEAL: &str = "Mícheál Connolly";
const ROISIN: &str = "Róisín Connolly";
/// How `/people` and the nearby list name Peig before she is introduced.
const PEIG_LOOKS: &str = "a sharp-eyed woman with a satchel of letters";

/// Walks to `place`, which must succeed.
fn go(game: &Game, place: &str) {
    let moved = game.submit(&format!("go to {place}"));
    assert_eq!(moved["terminalOutcome"], "succeeded", "{place}: {moved}");
}

fn wait(game: &Game, minutes: u32) {
    command(game, &format!("/wait {minutes}"));
}

/// The clock as `HH:MM`, from `/debug clock`.
fn clock(game: &Game) -> String {
    let view = command(game, "/debug clock");
    let rest = view
        .split("Game time: ")
        .nth(1)
        .unwrap_or_else(|| panic!("no clock in {view}"));
    rest[..5].to_string()
}

/// Sends `text` and answers the intent call as speech to `who`, returning
/// the pending dialogue call. The caller resolves it.
fn say(game: &Game, who: &str, text: &str) -> Value {
    game.submit(text);
    for _ in 0..4 {
        let pending = game.pending();
        assert!(!pending.is_null(), "{text}: no pending call");
        match pending["endpoint"]["role"].as_str() {
            Some("dialogue") => return pending,
            Some("intent") => {
                let option = option_named(&pending["input"], "present", who);
                game.resolve(&pending, talk_decision(&option, 0.95));
            }
            other => panic!("unexpected Endpoint role {other:?}"),
        }
    }
    panic!("{text}: no dialogue call for {who}");
}

/// Says `text` to `who`, answers with `reply`, and returns the dialogue
/// input the Endpoint was given: what `who` knew when answering.
fn hear(game: &Game, who: &str, text: &str, reply: &str) -> Value {
    let pending = say(game, who, text);
    assert_eq!(pending["input"]["aboutYou"]["name"], who);
    let input = pending["input"].clone();
    let done = game.resolve(&pending, json!({ "dialogue": reply }));
    assert_eq!(done["terminalOutcome"], "succeeded", "{done}");
    input
}

fn stranger(input: &Value) -> String {
    input["thisMoment"]["theStranger"]
        .as_str()
        .unwrap()
        .to_string()
}

fn gossip_of(game: &Game, who: &str) -> String {
    command(game, &format!("/debug gossip {who}"))
}

fn people_here(game: &Game) -> Vec<String> {
    people(&game.snapshot())
}

mod memory {
    use super::*;

    /// P6-01: the player gives a name to Mícheál. He remembers it as state
    /// (`/debug memory`), it reaches his next dialogue input, a relaunch
    /// keeps it, and Róisín, who was not told, still sees a stranger. A
    /// repeat on a new game gives the same before and after.
    #[test]
    fn a_name_given_to_one_npc_is_remembered_by_that_npc_across_a_relaunch() {
        for run in 1..=2 {
            let (mut game, _) = Game::new();
            game.go_to_the_cottage();

            let before = hear(&game, MICHEAL, "Good morning to you.", "And to you.");
            assert!(
                stranger(&before).starts_with("A stranger you have never seen"),
                "run {run}: {}",
                stranger(&before)
            );
            assert!(!command(&game, "/debug memory Mícheál").contains("Seán"));

            hear(
                &game,
                MICHEAL,
                "My name is Seán and I am looking for work droving yearlings.",
                "Is that so, Seán? I might have work for you.",
            );
            let memory = command(&game, "/debug memory Mícheál");
            assert!(memory.contains("Seán"), "run {run}: {memory}");

            let after = hear(&game, MICHEAL, "Do you remember my name?", "I do, Seán.");
            assert!(stranger(&after).contains("Seán"), "run {run}: {after}");

            game.relaunch();
            assert!(command(&game, "/debug memory Mícheál").contains("Seán"));
            let resumed = hear(&game, MICHEAL, "And what would you pay?", "A fair price.");
            assert!(
                stranger(&resumed).contains("Seán"),
                "run {run}: the memory survives the relaunch: {resumed}"
            );

            let roisin = hear(&game, ROISIN, "Good morning to you.", "And to you.");
            assert!(
                !stranger(&roisin).contains("Seán"),
                "run {run}: Róisín was not told: {roisin}"
            );
        }
    }
}

mod gossip {
    use super::*;

    /// P6-02: before propagation only the source knows the fact; Peig and
    /// Róisín answer without it, and a relaunch changes nothing.
    #[test]
    fn the_fact_starts_with_its_source_alone_and_nobody_else_acts_on_it() {
        for run in 1..=2 {
            let (mut game, _) = Game::new();
            for _ in 0..2 {
                let overview = command(&game, "/debug gossip");
                assert!(overview.contains(CATTLE_BUYER), "run {run}: {overview}");
                assert!(overview.contains("source=NPC#2, known_by=1"), "{overview}");
                assert!(gossip_of(&game, "Mícheál").contains(CATTLE_BUYER));
                assert!(gossip_of(&game, "Róisín").contains("(0 items)"));
                assert!(gossip_of(&game, "Peig").contains("(0 items)"));
                game.relaunch();
            }

            // The player goes straight to the cottage, so the Connollys are
            // together only while the player is one step away for the single
            // gossip roll of this first turn (seeded by the game hour, so
            // it is the same in every run).
            go(&game, "Connolly Cottage");
            let roisin = hear(&game, ROISIN, "Good morning to you.", "And to you.");
            assert!(
                !roisin.to_string().contains(CATTLE_BUYER),
                "run {run}: Róisín has not heard it yet: {roisin}"
            );
            go(&game, "Kilteevan Village");
            wait(&game, 60);
            let peig = hear(&game, PEIG, "Good morning to you.", "And to you.");
            assert!(
                !peig.to_string().contains(CATTLE_BUYER),
                "run {run}: Peig is not told: {peig}"
            );
        }
    }

    /// P6-03: with the player away from the cottage, the fact
    /// spreads from Mícheál to Róisín as saved state and to no one
    /// else. Róisín then shows it in her next reply's context, the save
    /// keeps it, and Peig is never told. A repeat on a new game
    /// spreads it again.
    #[test]
    fn the_fact_reaches_roisin_from_micheal_and_never_peig() {
        for run in 1..=2 {
            let (mut game, _) = Game::new();
            // The player stays in the village, away from the cottage.
            let mut hours = 0;
            while !gossip_of(&game, "Róisín").contains(CATTLE_BUYER) {
                hours += 1;
                assert!(
                    hours <= 24,
                    "run {run}: the fact never reached Róisín in a game day"
                );
                wait(&game, 60);
            }
            assert!(gossip_of(&game, "Mícheál").contains(CATTLE_BUYER));
            assert!(gossip_of(&game, "Peig").contains("(0 items)"));

            game.relaunch();
            assert!(gossip_of(&game, "Róisín").contains(CATTLE_BUYER));
            assert!(gossip_of(&game, "Peig").contains("(0 items)"));

            go(&game, "Connolly Cottage");
            let roisin = hear(&game, ROISIN, "Any news on the road?", "A buyer was about.");
            assert!(
                roisin["aboutYou"]["knows"]
                    .to_string()
                    .contains(CATTLE_BUYER),
                "run {run}: Róisín's context carries the fact: {roisin}"
            );
            go(&game, "Letter Office");
            // Peig is in the village from 08:00 to 09:00, so how long the
            // spread took decides whether she is at her counter yet.
            let mut waits = 0;
            while people_here(&game).is_empty() {
                waits += 1;
                assert!(waits <= 12, "run {run}: Peig never reached her counter");
                wait(&game, 15);
            }
            let peig = hear(&game, PEIG, "Any news on the road?", "None that I know.");
            assert!(
                !peig.to_string().contains(CATTLE_BUYER),
                "run {run}: Peig is never told: {peig}"
            );
        }
    }
}

mod movement {
    use super::*;

    /// P6-05: Peig walks Letter Office to village at 07:00 and back at
    /// 09:00. `/people`, who can be spoken to, and the save agree at each
    /// point, including a save made on the walk out (07:00, before she
    /// has left) and one made on the walk back (09:01). A repeat on a new
    /// game gives the same sequence.
    #[test]
    fn peig_is_in_the_village_from_eight_to_nine_and_the_save_agrees() {
        for run in 1..=2 {
            let (mut game, _) = Game::new();
            assert_eq!(clock(&game), "07:00");
            assert!(
                command(&game, "/debug schedule Peig").contains("07:00-08:00  Kilteevan Village"),
                "run {run}"
            );
            assert!(
                people_here(&game).is_empty(),
                "run {run}: she is still on the way out at 07:00"
            );

            // A save made on the walk out resumes mid-walk, not in either place.
            game.relaunch();
            assert_eq!(clock(&game), "07:00", "run {run}: resumed on the walk out");
            assert!(people_here(&game).is_empty(), "run {run}: after relaunch");

            wait(&game, 60);
            assert_eq!(clock(&game), "08:00");
            assert_eq!(people_here(&game), [PEIG_LOOKS], "run {run}");
            assert!(command(&game, "/people").contains("satchel of letters"));
            let input = hear(&game, PEIG, "Good morning to you.", "And to you.");
            assert_eq!(input["thisMoment"]["where"], "Kilteevan Village");

            game.relaunch();
            assert_eq!(
                people_here(&game),
                [PEIG_LOOKS],
                "run {run}: relaunch at 08:00"
            );

            wait(&game, 59);
            assert_eq!(clock(&game), "08:59");
            assert_eq!(people_here(&game), [PEIG_LOOKS], "run {run}");

            // 09:01: she has left and is on the lane, in neither place.
            wait(&game, 2);
            assert_eq!(clock(&game), "09:01");
            assert!(people_here(&game).is_empty(), "run {run}: walking back");
            assert!(command(&game, "/people").contains("No one else is here"));
            game.submit("Good morning to you.");
            let pending = game.pending();
            assert!(
                pending.is_null() || pending["endpoint"]["role"] == "intent",
                "{pending}"
            );
            if !pending.is_null() {
                assert!(
                    pending["input"]["present"].as_array().unwrap().is_empty(),
                    "nobody can be spoken to: {pending}"
                );
                game.resolve(&pending, talk_decision("no_one", 0.95));
            }

            game.relaunch();
            assert_eq!(clock(&game), "09:01", "run {run}: the save was mid-walk");
            assert!(people_here(&game).is_empty(), "run {run}: after relaunch");

            wait(&game, 10);
            assert!(people_here(&game).is_empty());
            go(&game, "Letter Office");
            assert_eq!(
                people_here(&game),
                [PEIG_LOOKS],
                "run {run}: at her counter"
            );
            let input = hear(&game, PEIG, "Where were you this morning?", "On the road.");
            assert_eq!(input["thisMoment"]["where"], "Letter Office");

            game.relaunch();
            assert_eq!(people_here(&game), [PEIG_LOOKS], "run {run}");
        }
    }
}

#[cfg(feature = "living-world-pending")]
mod weather {
    use super::*;

    /// P6-04 (#2198): in heavy rain Peig does not walk to the village at
    /// 07:00. At 08:00 the village is empty, she is at the Letter Office,
    /// and the save agrees. In clear weather the same game minute puts her
    /// in the village. A repeat gives the same outcomes.
    #[test]
    fn heavy_rain_keeps_peig_at_the_letter_office() {
        for run in 1..=2 {
            for (weather, in_village) in [("heavy rain", false), ("clear", true)] {
                let (mut game, _) = Game::new();
                assert!(
                    command(&game, &format!("/weather {weather}")).contains("The weather shifts"),
                    "run {run}"
                );
                wait(&game, 60);
                assert_eq!(clock(&game), "08:00");
                assert_eq!(
                    people_here(&game).is_empty(),
                    !in_village,
                    "run {run}, {weather}: {:?}",
                    people_here(&game)
                );
                game.relaunch();
                assert_eq!(people_here(&game).is_empty(), !in_village, "run {run}");

                if !in_village {
                    go(&game, "Letter Office");
                    assert_eq!(people_here(&game), [PEIG_LOOKS], "run {run}, {weather}");
                    let input = hear(&game, PEIG, "Terrible weather.", "It is that.");
                    assert_eq!(input["thisMoment"]["where"], "Letter Office");
                    assert!(
                        input["thisMoment"]["weather"]
                            .as_str()
                            .unwrap()
                            .contains("Rain"),
                        "{input}"
                    );
                }
            }
        }
    }
}

#[cfg(feature = "living-world-pending")]
mod task {
    use super::*;

    /// P6-06 (#2308): Róisín asks the player to have Peig hold the
    /// Connollys' letters. The task is assigned, in progress at the Letter
    /// Office, then completed by telling Peig, each as saved state shown by
    /// `/debug tasks`; the save keeps each stage and, afterwards, Peig
    /// speaks of the message as delivered. A repeat gives the same stages.
    #[test]
    fn the_task_is_assigned_advanced_and_completed_through_the_world() {
        for run in 1..=2 {
            let (mut game, _) = Game::new();
            game.go_to_the_cottage();
            assert!(
                command(&game, "/debug tasks").contains("(no tasks)"),
                "run {run}"
            );

            hear(
                &game,
                ROISIN,
                "Is there anything I can do for you?",
                "Would you tell Peig Hannigan at the Letter Office to hold any letter that comes for the Connollys until I call for it?",
            );
            hear(&game, ROISIN, "I will tell her.", "God bless you for it.");
            let assigned = command(&game, "/debug tasks");
            assert!(
                assigned.contains("hold any letter") && assigned.contains("assigned"),
                "run {run}: {assigned}"
            );

            game.relaunch();
            assert!(
                command(&game, "/debug tasks").contains("assigned"),
                "run {run}"
            );

            wait(&game, 150);
            go(&game, "Kilteevan Village");
            go(&game, "Letter Office");
            assert!(
                command(&game, "/debug tasks").contains("in progress"),
                "run {run}"
            );

            hear(
                &game,
                PEIG,
                "Róisín asks you to hold any letter that comes for the Connollys until she calls for it.",
                "I will hold them.",
            );
            assert!(
                command(&game, "/debug tasks").contains("completed"),
                "run {run}"
            );
            game.relaunch();
            assert!(
                command(&game, "/debug tasks").contains("completed"),
                "run {run}"
            );

            let peig = hear(&game, PEIG, "Did you get Róisín's message?", "I did.");
            assert!(
                peig.to_string().contains("hold any letter"),
                "run {run}: Peig speaks of it as delivered: {peig}"
            );
            go(&game, "Kilteevan Village");
            go(&game, "Connolly Cottage");
            let roisin = hear(&game, ROISIN, "I told Peig.", "Thank you.");
            assert!(
                roisin.to_string().contains("hold any letter"),
                "run {run}: Róisín speaks of it as delivered: {roisin}"
            );
        }
    }
}
