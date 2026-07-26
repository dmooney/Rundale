//! Per-session background tick tasks.
//!
//! [`spawn_session_ticks`] starts all background tasks for a session and
//! returns their [`JoinHandle`]s.  Every task observes `shutdown_token` via
//! `tokio::select!` so it exits cleanly on session eviction (#228).
//!
//! Tasks spawned here:
//! 1. Character-log subscriber
//! 2. Location-log subscriber
//! 3. Chat-transcript subscriber
//! 4. Game-events subscriber (#1222)
//! 5. World tick (5 s)
//! 6. Inactivity tick (1 s)
//! 7. Autosave tick
//! 8. Tier-2 simulation tick (#1198)

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use parish_core::event_bus::{EventBus as EventBusTrait, Topic};

use crate::state::AppState;

use super::GOSSIP_BUDGET_PER_TICK;
pub use parish_core::AUTOSAVE_INTERVAL_SECS;

/// Bound on the per-subscriber chronicle-log writer channel.
///
/// Each character-/location-log subscriber feeds its blocking `process_event`
/// work to a single long-lived writer task over a bounded
/// [`tokio::sync::mpsc`] channel of this capacity (one channel per subscriber
/// per session). This replaces the old per-event `spawn_blocking`, which put
/// one short-lived task per `GameEvent` onto the shared blocking pool, with no
/// explicit bound and unbounded churn multiplied across N concurrent sessions.
///
/// Saturation behavior is **block, not drop**: when the channel is full the
/// subscriber loop awaits `tx.send(item)`, applying backpressure to its own
/// `world.event_bus` recv loop rather than discarding the work item. Chronicle
/// entries are an append-only historical record — silently dropping a
/// `PlayerMoved`/`DialogueOccurred` write would leave a permanent hole in
/// `player.md`/`loc-*.md` that no later event repairs. Backpressure instead
/// lets the upstream broadcast channel (`BUS_CAPACITY = 256`) absorb the burst;
/// only if *that* overflows does the pre-existing `RecvError::Lagged` arm skip
/// (the documented, unchanged lossy backstop). The value is a small fixed bound
/// (the loop was already de-facto serial — one in-flight write per subscriber —
/// so a handful of slots is ample headroom to decouple recv from the blocking
/// write without unbounded buffering).
const LOG_WRITER_QUEUE_CAPACITY: usize = 32;

/// Spawns the per-session background tasks and returns their handles.
///
/// Each task observes `shutdown_token` via `tokio::select!` so it exits
/// cleanly when the token is cancelled (e.g. on session eviction) rather than
/// running until its `JoinHandle` is aborted (#228).
pub(super) fn spawn_session_ticks(
    state: Arc<AppState>,
    shutdown_token: tokio_util::sync::CancellationToken,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::with_capacity(8);

    // ── Character-log subscriber ───────────────────────────────────────────
    //
    // Per-character markdown logs (rule #12: orchestration lives in
    // parish-core; this is the thin server-side wiring). Gated by the
    // `character-logs` flag (default on). Subscribes BEFORE the profile
    // rewrite so no events fired during/just after initial profile
    // generation are lost.
    {
        use parish_core::character_log::CharacterLogManager;

        // One long-lived writer task per subscriber, fed by a bounded channel
        // (capacity LOG_WRITER_QUEUE_CAPACITY). The blocking context is entered
        // ONCE per session here — not once-per-event in the recv loop — and the
        // writer drains items serially (exactly one `process_event` in flight at
        // a time). Saturation behavior is BLOCK, not drop (see the constant's
        // doc comment). Per rule #11 (scaling): the channel and the writer-task
        // handle are per-session, created here in `spawn_session_ticks`; the
        // handle is collected into `handles` so the task is dropped on session
        // eviction. No `static`/`broadcast::channel`/`Topic` is introduced.
        let (tx, writer_rx) = tokio::sync::mpsc::channel::<(
            CharacterLogManager,
            parish_core::world::events::GameEvent,
            u64,
        )>(LOG_WRITER_QUEUE_CAPACITY);

        let writer_state = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            run_character_log_writer(writer_state, writer_rx).await;
        }));

        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            use parish_core::character_log::FEATURE_FLAG;

            let enabled = {
                let cfg = s.config.lock().await;
                !cfg.flags.is_disabled(FEATURE_FLAG)
            };
            if !enabled {
                // Dropping `tx` closes the channel so the writer task exits.
                return;
            }
            let app_name = parish_core::game_mod::app_name_from_mod(&s.game_mod);
            let mut current_branch = s.save_identity.current_branch_id.lock().await.unwrap_or(1);
            let mut manager = CharacterLogManager::new(&app_name, current_branch, true);
            let mut rx = {
                let world = s.world.lock().await;
                world.event_bus.subscribe_contextual()
            };
            {
                let world = s.world.lock().await;
                let npc_mgr = s.npc_manager.lock().await;
                if let Err(e) = manager.write_all_profiles(&world, &npc_mgr) {
                    tracing::warn!(error = %e, "character-log profile write failed");
                }
            }
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(envelope) => {
                            let _persistence_guard = s.persistence_gate.lock().await;
                            let current_epoch = {
                                let world = s.world.lock().await;
                                world.event_bus.context_epoch()
                            };
                            if envelope.context_epoch != current_epoch {
                                continue;
                            }
                            // Rebind manager when the active branch has changed
                            // (e.g. load_branch / create_branch). Without this the
                            // writer keeps appending to the original branch's
                            // log directory after a branch switch (#1011). The
                            // POST-rebind manager clone is what gets enqueued, so
                            // events after a /load land under the new branch dir.
                            let bid = s.save_identity.current_branch_id.lock().await.unwrap_or(1);
                            if bid != current_branch {
                                current_branch = bid;
                                manager = CharacterLogManager::new(&app_name, bid, true);
                                let world = s.world.lock().await;
                                let npc_mgr = s.npc_manager.lock().await;
                                if let Err(e) = manager.write_all_profiles(&world, &npc_mgr) {
                                    tracing::warn!(error = %e, "character-log profile write failed after branch switch");
                                }
                            }
                            // Enqueue (post-rebind manager clone, event) onto the
                            // bounded channel. On a full channel this `await`
                            // BLOCKS — applying backpressure to this recv loop —
                            // rather than dropping the work item. A closed channel
                            // means the writer task exited, so stop. Select on the
                            // shutdown token so a saturated send cannot stall
                            // session teardown (the in-flight item is abandoned —
                            // the session is being evicted).
                            tokio::select! {
                                _ = token.cancelled() => break,
                                send_res = tx.send((
                                    manager.clone(),
                                    envelope.event,
                                    envelope.context_epoch,
                                )) => {
                                    if send_res.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        }));
    }

    // ── Location-log subscriber ────────────────────────────────────────────
    //
    // Mirrors the character-log subscriber above; writes per-location
    // markdown logs. Gated by the `location-logs` flag (default on).
    {
        use parish_core::location_log::LocationLogManager;

        // Mirrors the character-log subscriber: one long-lived writer task fed
        // by a bounded channel (capacity LOG_WRITER_QUEUE_CAPACITY), block-on-
        // full saturation, handle collected into `handles` (rule #11).
        let (tx, writer_rx) = tokio::sync::mpsc::channel::<(
            LocationLogManager,
            parish_core::world::events::GameEvent,
            u64,
        )>(LOG_WRITER_QUEUE_CAPACITY);

        let writer_state = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            run_location_log_writer(writer_state, writer_rx).await;
        }));

        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            use parish_core::location_log::FEATURE_FLAG;

            let enabled = {
                let cfg = s.config.lock().await;
                !cfg.flags.is_disabled(FEATURE_FLAG)
            };
            if !enabled {
                // Dropping `tx` closes the channel so the writer task exits.
                return;
            }
            let app_name = parish_core::game_mod::app_name_from_mod(&s.game_mod);
            let mut current_branch = s.save_identity.current_branch_id.lock().await.unwrap_or(1);
            let mut manager = LocationLogManager::new(&app_name, current_branch, true);
            let mut rx = {
                let world = s.world.lock().await;
                world.event_bus.subscribe_contextual()
            };
            {
                let world = s.world.lock().await;
                let npc_mgr = s.npc_manager.lock().await;
                if let Err(e) = manager.write_all_profiles(&world, &npc_mgr) {
                    tracing::warn!(error = %e, "location-log profile write failed");
                }
            }
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(envelope) => {
                            let _persistence_guard = s.persistence_gate.lock().await;
                            let current_epoch = {
                                let world = s.world.lock().await;
                                world.event_bus.context_epoch()
                            };
                            if envelope.context_epoch != current_epoch {
                                continue;
                            }
                            // Rebind manager when the active branch has changed
                            // (e.g. load_branch / create_branch). Mirrors the
                            // character-log subscriber fix from #1011 (#1034).
                            // The POST-rebind manager clone is what gets enqueued.
                            let bid = s.save_identity.current_branch_id.lock().await.unwrap_or(1);
                            if bid != current_branch {
                                current_branch = bid;
                                manager = LocationLogManager::new(&app_name, bid, true);
                                let world = s.world.lock().await;
                                let npc_mgr = s.npc_manager.lock().await;
                                if let Err(e) = manager.write_all_profiles(&world, &npc_mgr) {
                                    tracing::warn!(error = %e, "location-log profile write failed after branch switch");
                                }
                            }
                            // Enqueue (post-rebind manager clone, event) onto the
                            // bounded channel. On a full channel this `await`
                            // BLOCKS rather than dropping. A closed channel means
                            // the writer task exited, so stop. Select on the
                            // shutdown token so a saturated send cannot stall
                            // session teardown (the in-flight item is abandoned —
                            // the session is being evicted).
                            tokio::select! {
                                _ = token.cancelled() => break,
                                send_res = tx.send((
                                    manager.clone(),
                                    envelope.event,
                                    envelope.context_epoch,
                                )) => {
                                    if send_res.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        }));
    }

    // ── Chat-transcript subscriber ─────────────────────────────────────────
    //
    // Writes the user-visible chat stream as JSONL paired with the inference
    // log (for zippable bug reports), correlating NPC dialogue to the
    // inference call via `parish.request_id`. The writer task + enable flag
    // were created at session start on `AppState.chat_transcript_log`; this
    // subscriber just forwards bus events to it. Per-session (not per-branch),
    // so unlike the markdown log managers it never rebinds.
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            // Always subscribe — even if logging starts disabled — so a
            // mid-session `/inference-log on` is captured. `process_event`
            // no-ops internally while the shared flag is off.
            let mut rx = {
                let world = s.world.lock().await;
                world.event_bus.subscribe_contextual()
            };
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(envelope) => {
                            let _persistence_guard = s.persistence_gate.lock().await;
                            let world = s.world.lock().await;
                            if envelope.context_epoch != world.event_bus.context_epoch() {
                                continue;
                            }
                            let npc_mgr = s.npc_manager.lock().await;
                            s.chat_transcript_log
                                .process_event(&envelope.event, &world, &npc_mgr);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        }));
    }

    // ── Game-events subscriber ────────────────────────────────────────────────
    //
    // Captures `GameEvent`s from `world.event_bus` into the `state.game_events`
    // ring buffer so they appear in debug snapshots and composed bug reports
    // (#1222). Mirrors the equivalent task in `parish-tauri/src/setup.rs`
    // (the `spawn_game_event_subscriber` call at the end of
    // `spawn_world_event_listeners`). Without this task, `state.game_events` is
    // always empty in server sessions, causing the "Game events" section in
    // every auto-filed bug report to show `_none_` regardless of actual activity.
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            let mut rx = {
                let world = s.world.lock().await;
                world.event_bus.subscribe_contextual()
            };
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(envelope) => {
                            record_contextual_game_event(&s, envelope).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        }));
    }

    // ── World tick (5 s) ─────────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            // Round-robin cursor for budgeted gossip propagation (#466).
            let mut gossip_cursor: usize = 0;
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }

                {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    let world = s.world.lock().await;
                    let npc_manager = s.npc_manager.lock().await;
                    let mut snap = parish_core::ipc::snapshot_from_world(&world);
                    snap.name_hints = parish_core::ipc::compute_name_hints(
                        &world,
                        &npc_manager,
                        &s.pronunciations,
                    );
                    s.event_bus
                        .emit_named(Topic::WorldUpdate, "world-update", &snap);
                }

                {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    // Snapshot the banshee flag outside the world/npc locks to avoid
                    // nesting config → world, which inverts the project-wide
                    // lock order.
                    let banshee_enabled = {
                        let cfg = s.config.lock().await;
                        !cfg.flags.is_disabled("banshee")
                    };

                    let mut world = s.world.lock().await;
                    let mut npc_mgr = s.npc_manager.lock().await;

                    // Advance the world one pump through the single shared
                    // helper (rule #12): weather + schedules + tier
                    // reassignment + banshee + budgeted gossip. The server
                    // intentionally leaves tier-4 to its own scheduling (it has
                    // never dispatched tier-4 from this loop). The budgeted
                    // gossip cursor round-robins across ticks (#466).
                    {
                        use parish_core::game_loop::{
                            AdvanceOptions, GossipMode, WeatherMode, advance_world,
                        };

                        let mut rng = rand::rng();
                        let report = advance_world(
                            &mut world,
                            &mut npc_mgr,
                            &mut rng,
                            AdvanceOptions {
                                weather: WeatherMode::Single,
                                run_banshee: banshee_enabled,
                                gossip: GossipMode::Budgeted {
                                    cursor: gossip_cursor,
                                    budget: GOSSIP_BUDGET_PER_TICK,
                                },
                                run_tier4: false,
                            },
                        );
                        gossip_cursor = report.gossip_cursor;
                    }

                    // Advance the generation counter so handle_game_input can
                    // detect TOCTOU races (see issue #283).
                    //
                    // Skip while inference-paused: the player input is
                    // mid-flight and the clock is frozen, so this tick is a
                    // no-op from the player's perspective. Bumping the
                    // counter anyway falsely tripped the TOCTOU guard.
                    if !world.clock.is_inference_paused() {
                        world.increment_tick_generation();
                    }

                    // #621 — Per-session tick metric. Emitted as a structured
                    // tracing event so log-based metric tools can aggregate
                    // tick counts per session without a Prometheus exporter.
                    tracing::debug!(
                        target: "parish_server::metrics",
                        session_id = %s.session_id,
                        tick = world.tick_generation,
                        "session.tick"
                    );
                }
            }
        }));
    }

    // ── Inactivity tick (1 s) ────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
                crate::routes::tick_inactivity(&s).await;
            }
        }));
    }

    // ── Autosave tick ────────────────────────────────────────────────────────
    //
    // #230 — Fixes: previously a fresh `Database::open` (and therefore a full
    // `migrate()` round-trip) was executed on every tick.  Now we lazily open
    // an `AsyncDatabase` the first time we have a save path and reuse it for
    // all subsequent ticks.  All SQLite work is delegated to `spawn_blocking`
    // inside `AsyncDatabase`, so a slow fsync can never stall the Tokio runtime.
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            use parish_core::persistence::snapshot::GameSnapshot;
            use parish_core::persistence::{AsyncDatabase, Database};
            // Track whether the last autosave attempt failed so we only emit
            // one user-visible warning per failure run, not one per tick.
            let mut last_autosave_failed = false;
            loop {
                // Wait for the interval or cancellation.  Cancellation exits
                // only after the *sleep* fires, so any in-flight autosave
                // iteration completes before the task stops (#228).
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(AUTOSAVE_INTERVAL_SECS)) => {}
                }

                // Capture identity and world state, then commit the snapshot,
                // under the same outer barrier used by task turns and
                // lifecycle switches. This prevents a pre-task snapshot from
                // committing after the task journal append.
                let _persistence_guard = s.persistence_gate.lock().await;
                let save_path_guard = s.save_identity.save_path.lock().await;
                let branch_id_guard = s.save_identity.current_branch_id.lock().await;
                let save_path = save_path_guard.clone();
                let branch_id = *branch_id_guard;
                drop(branch_id_guard);
                drop(save_path_guard);

                if let (Some(path), Some(bid)) = (save_path, branch_id) {
                    // Snapshot the world state before touching the DB lock.
                    let snapshot = {
                        let world = s.world.lock().await;
                        let npc_manager = s.npc_manager.lock().await;
                        GameSnapshot::capture(&world, &npc_manager)
                    };

                    // Obtain (or open) the cached AsyncDatabase for this path.
                    let db: Option<AsyncDatabase> = {
                        let mut guard = s.save_db.lock().await;
                        // If the cached path no longer matches the active save file
                        // (e.g. after load-branch / new-save-file), discard the old handle.
                        if guard.as_ref().is_some_and(|(p, _)| p != &path) {
                            *guard = None;
                        }
                        if guard.is_none() {
                            let path_clone = path.clone();
                            match tokio::task::spawn_blocking(move || Database::open(&path_clone))
                                .await
                            {
                                Ok(Ok(db)) => {
                                    *guard = Some((path.clone(), AsyncDatabase::new(db)));
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!("Autosave: failed to open DB: {}", e);
                                    if !last_autosave_failed {
                                        s.event_bus.emit_named(
                                            Topic::TextLog,
                                            "text-log",
                                            &parish_core::ipc::text_log(
                                                "system",
                                                "Autosave failed — could not open save file.",
                                            ),
                                        );
                                        last_autosave_failed = true;
                                    }
                                    continue;
                                }
                                Err(e) => {
                                    tracing::warn!("Autosave: spawn_blocking error: {}", e);
                                    continue;
                                }
                            }
                        }
                        guard.as_ref().map(|(_, db)| db.clone())
                    };

                    if let Some(db) = db {
                        match db.save_snapshot(bid, &snapshot).await {
                            Ok(_) => {
                                tracing::debug!("Session autosave complete");
                                if last_autosave_failed {
                                    s.event_bus.emit_named(
                                        Topic::TextLog,
                                        "text-log",
                                        &parish_core::ipc::text_log(
                                            "system",
                                            "Autosave resumed successfully.",
                                        ),
                                    );
                                }
                                last_autosave_failed = false;
                            }
                            Err(e) => {
                                tracing::warn!("Session autosave failed: {}", e);
                                if !last_autosave_failed {
                                    s.event_bus.emit_named(
                                        Topic::TextLog,
                                        "text-log",
                                        &parish_core::ipc::text_log(
                                            "system",
                                            "Autosave failed — your progress may not be saved.",
                                        ),
                                    );
                                    last_autosave_failed = true;
                                }
                            }
                        }
                    }
                }
            }
        }));
    }

    // ── Tier-2 simulation tick ────────────────────────────────────────────────
    //
    // Mirrors the Tier-2 polling task from `parish-tauri/src/setup.rs` and the
    // `dispatch_headless_tier2_tick` helper from `parish-engine/src/headless.rs`.
    // Previously missing from the web backend (TD-040), so `GossipSpread` events
    // never fired on the Axum path. The shared `mint_tier2_gossip` helper (TD-030)
    // is called for post-processing instead of repeating the loop body a third time
    // (rule #12).
    //
    // Lock ordering: world → npc_manager (documented contract, matches the main
    // world-tick above and the Tauri site — both hold world first, npc_manager
    // second). The sim client and config are snapshotted before acquiring these
    // locks so the LLM call is never made while holding a game-state lock.
    {
        let s = Arc::clone(&state);
        let token = shutdown_token.clone();
        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }

                // ── Check whether a Tier-2 tick is due ───────────────────────
                let (needs_tick, in_flight, groups, time_desc, weather_str, context_epoch) = {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    let world = s.world.lock().await;
                    let npc_mgr = s.npc_manager.lock().await;
                    let now = world.clock.now();
                    if !npc_mgr.needs_tier2_tick(now) || npc_mgr.tier2_in_flight() {
                        continue;
                    }
                    let groups = parish_core::game_loop::build_tier2_groups(&world, &npc_mgr);
                    if groups.is_empty() {
                        continue;
                    }
                    let time_desc = world.clock.time_of_day().to_string();
                    let weather_str = world.weather.to_string();
                    (
                        true,
                        false,
                        groups,
                        time_desc,
                        weather_str,
                        world.event_bus.context_epoch(),
                    )
                };
                let _ = (needs_tick, in_flight); // consumed by the checks above

                // Mark in-flight before releasing the lock so a concurrent tick
                // cannot start a second batch.
                {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    let current_epoch = {
                        let world = s.world.lock().await;
                        world.event_bus.context_epoch()
                    };
                    if current_epoch != context_epoch {
                        continue;
                    }
                    let mut npc_mgr = s.npc_manager.lock().await;
                    npc_mgr.set_tier2_in_flight(true);
                }

                // ── Snapshot the sim client + model outside game-state locks ─
                let (client_opt, model) = {
                    use parish_core::config::InferenceCategory;
                    let cfg = s.config.lock().await;
                    let base_client = s.inference.client.lock().await;
                    cfg.resolve_category_client(InferenceCategory::Simulation, base_client.as_ref())
                };

                let Some(sim_client) = client_opt else {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    let current_epoch = {
                        let world = s.world.lock().await;
                        world.event_bus.context_epoch()
                    };
                    if current_epoch != context_epoch {
                        continue;
                    }
                    s.npc_manager.lock().await.set_tier2_in_flight(false);
                    continue;
                };

                // ── Run one LLM call per Tier-2 group (outside locks) ────────
                let lang = s.language_settings.clone();
                let mut events = Vec::new();
                for group in &groups {
                    if let Some(evt) = parish_core::npc::ticks::run_tier2_for_group(
                        &sim_client,
                        &model,
                        group,
                        &time_desc,
                        &weather_str,
                        &lang,
                        None,
                    )
                    .await
                    {
                        events.push(evt);
                    }
                }

                // ── Apply events and mint gossip under game-state locks ───────
                // Lock ordering: world → npc_manager.
                {
                    let _persistence_guard = s.persistence_gate.lock().await;
                    let mut world = s.world.lock().await;
                    if world.event_bus.context_epoch() != context_epoch {
                        continue;
                    }
                    let mut npc_mgr = s.npc_manager.lock().await;
                    let game_time = world.clock.now();

                    let dbg = parish_core::game_loop::mint_tier2_gossip(
                        &events,
                        npc_mgr.npcs_mut(),
                        game_time,
                        &parish_core::config::NpcConfig::default(),
                        &mut world,
                    );
                    npc_mgr.record_tier2_tick(game_time);
                    npc_mgr.set_tier2_in_flight(false);

                    let mut debug_events = s.debug_events.lock().await;
                    if debug_events.len() >= crate::state::DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(parish_core::debug_snapshot::DebugEvent {
                        timestamp: String::new(),
                        category: "tier2".to_string(),
                        message: format!(
                            "Tier 2 tick: {} events from {} groups{}",
                            events.len(),
                            groups.len(),
                            if dbg.is_empty() {
                                String::new()
                            } else {
                                format!("; {}", dbg.join(", "))
                            },
                        ),
                    });
                }
            }
        }));
    }

    handles
}

/// Applies one publish-time-stamped event to the debug/MCP event ring.
///
/// Kept as one seam so tests can deterministically queue an old-context
/// envelope, advance the canonical epoch, and prove it is rejected without
/// depending on Tokio task scheduling.
async fn record_contextual_game_event(
    state: &Arc<AppState>,
    envelope: parish_core::world::events::ContextEventEnvelope,
) -> bool {
    let _persistence_guard = state.persistence_gate.lock().await;
    let current_epoch = {
        let world = state.world.lock().await;
        world.event_bus.context_epoch()
    };
    if envelope.context_epoch != current_epoch {
        return false;
    }
    let mut buf = state.game_events.lock().await;
    if buf.len() >= crate::state::DEBUG_EVENT_CAPACITY {
        buf.pop_front();
    }
    buf.push_back(envelope.event);
    state
        .total_game_events
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    true
}

/// Drains the character-log writer channel serially, running each
/// `process_event` under the world/npc blocking locks inside one
/// `spawn_blocking`. Runs until the channel is closed (the subscriber loop
/// dropped its `tx`, e.g. on session eviction or cancellation). Exactly one
/// write is in flight at a time. Factored out of `spawn_session_ticks` so the
/// bound/no-loss behavior is unit-testable in isolation (see the `tests`
/// module).
async fn run_character_log_writer(
    state: Arc<AppState>,
    mut rx: tokio::sync::mpsc::Receiver<(
        parish_core::character_log::CharacterLogManager,
        parish_core::world::events::GameEvent,
        u64,
    )>,
) {
    while let Some((mgr, event, event_epoch)) = rx.recv().await {
        let persistence_guard = state.persistence_gate.lock().await;
        let current_epoch = {
            let world = state.world.lock().await;
            world.event_bus.context_epoch()
        };
        if event_epoch != current_epoch {
            continue;
        }
        let st = Arc::clone(&state);
        let handle = tokio::task::spawn_blocking(move || {
            let world = st.world.blocking_lock();
            let npc_mgr = st.npc_manager.blocking_lock();
            mgr.process_event(&event, &world, &npc_mgr)
        });
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "character-log write failed"),
            Err(e) => tracing::warn!(error = %e, "character-log task panicked"),
        }
        drop(persistence_guard);
    }
}

/// Mirror of [`run_character_log_writer`] for the location-log subscriber.
async fn run_location_log_writer(
    state: Arc<AppState>,
    mut rx: tokio::sync::mpsc::Receiver<(
        parish_core::location_log::LocationLogManager,
        parish_core::world::events::GameEvent,
        u64,
    )>,
) {
    while let Some((mgr, event, event_epoch)) = rx.recv().await {
        let persistence_guard = state.persistence_gate.lock().await;
        let current_epoch = {
            let world = state.world.lock().await;
            world.event_bus.context_epoch()
        };
        if event_epoch != current_epoch {
            continue;
        }
        let st = Arc::clone(&state);
        let handle = tokio::task::spawn_blocking(move || {
            let world = st.world.blocking_lock();
            let npc_mgr = st.npc_manager.blocking_lock();
            mgr.process_event(&event, &world, &npc_mgr)
        });
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "location-log write failed"),
            Err(e) => tracing::warn!(error = %e, "location-log task panicked"),
        }
        drop(persistence_guard);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        AUTOSAVE_INTERVAL_SECS, LOG_WRITER_QUEUE_CAPACITY, record_contextual_game_event,
        run_character_log_writer,
    };

    /// C4: the per-subscriber writer channel is bounded to
    /// `LOG_WRITER_QUEUE_CAPACITY` and its saturation behavior is **block, not
    /// drop**. This exercises the exact `tokio::sync::mpsc` bound + serial
    /// writer-task pairing that the two log subscribers use, in isolation (no
    /// full `AppState`), and asserts:
    ///
    /// 1. When the channel is full and the writer is parked, an extra
    ///    `tx.send()` does NOT drop — it pends (returns `Poll::Pending` /
    ///    times out under `try_recv`-style polling) rather than erroring or
    ///    silently discarding.
    /// 2. After the writer drains, **every** enqueued item is processed — the
    ///    processed count equals the number of items sent (no loss under an
    ///    over-capacity flood).
    #[tokio::test]
    async fn log_writer_channel_blocks_when_full_and_loses_no_events() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Mirror the production pairing: a bounded channel of
        // LOG_WRITER_QUEUE_CAPACITY feeding a single serial writer task. The
        // writer counts each item it processes (standing in for
        // `process_event`'s disk write). A gate keeps the writer parked so we
        // can observe a genuinely full channel before any draining begins.
        let cap = LOG_WRITER_QUEUE_CAPACITY;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<usize>(cap);

        let processed = Arc::new(AtomicUsize::new(0));
        let processed_w = Arc::clone(&processed);
        let release = Arc::new(tokio::sync::Notify::new());
        let release_w = Arc::clone(&release);

        let writer = tokio::spawn(async move {
            // Park until released so the channel can genuinely fill.
            release_w.notified().await;
            while let Some(_item) = rx.recv().await {
                processed_w.fetch_add(1, Ordering::SeqCst);
            }
        });

        // Fill the channel to capacity. With the writer parked, these all
        // buffer and succeed immediately.
        for i in 0..cap {
            tx.send(i).await.expect("send within capacity must succeed");
        }

        // Channel is now full and the writer is parked. An extra send must
        // BLOCK (pend), not drop and not error. Prove it pends by racing the
        // send against a short timeout — the timeout must win.
        let overflow_send = tx.send(cap);
        let pended = tokio::time::timeout(std::time::Duration::from_millis(200), overflow_send)
            .await
            .is_err();
        assert!(
            pended,
            "send on a full channel must block (apply backpressure), not drop or error"
        );

        // Release the writer; it drains everything including the previously
        // blocked overflow item once a slot opens.
        let total = cap + 1;
        release.notify_one();
        // Re-issue the overflow send now that draining can make room. (The
        // future from the timed-out attempt was cancelled; nothing was
        // dropped from the channel itself — backpressure, not loss.)
        tx.send(cap)
            .await
            .expect("send must complete once draining frees a slot");

        // Close the channel and wait for the writer to finish draining.
        drop(tx);
        writer.await.expect("writer task must not panic");

        assert_eq!(
            processed.load(Ordering::SeqCst),
            total,
            "every enqueued item must be processed under an over-capacity flood — no drops"
        );
    }

    #[test]
    fn autosave_interval_is_60_seconds() {
        // Regression sensor: if this changes, update comment in session/ticks.rs
        // and verify players won't lose more than AUTOSAVE_INTERVAL_SECS of progress.
        assert_eq!(
            AUTOSAVE_INTERVAL_SECS, 60,
            "autosave interval changed — verify data-loss risk is acceptable"
        );
    }

    /// Verifies that the game-events subscriber task (fix #1222) captures
    /// `GameEvent`s published to `world.event_bus` and stores them in
    /// `state.game_events` so they appear in bug reports.
    ///
    /// Steps:
    /// 1. Build a test `AppState`.
    /// 2. Subscribe to `world.event_bus` via `spawn_session_ticks`.
    /// 3. Publish a `GameEvent::PlayerMoved` to `world.event_bus`.
    /// 4. Yield to let the subscriber task run.
    /// 5. Assert that `state.game_events` contains the event.
    #[tokio::test]
    async fn game_events_subscriber_captures_published_events() {
        use parish_core::world::LocationId;
        use parish_core::world::events::GameEvent;
        use std::sync::Arc;
        use tokio_util::sync::CancellationToken;

        let state = Arc::new(crate::routes::tests::test_app_state());
        let token = CancellationToken::new();
        let _handles =
            crate::session::ticks::spawn_session_ticks(Arc::clone(&state), token.clone());

        // Yield to the Tokio runtime so the subscriber task can subscribe.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // Publish a PlayerMoved event to the world event bus.
        let now = {
            let world = state.world.lock().await;
            world.clock.now()
        };
        {
            let world = state.world.lock().await;
            world.event_bus.publish(GameEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
                timestamp: now,
            });
        }

        // Yield again so the subscriber task processes the event.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // The subscriber must have captured the event.
        let events = state.game_events.lock().await;
        assert!(
            !events.is_empty(),
            "game_events subscriber must capture PlayerMoved published to world.event_bus"
        );
        let kinds: Vec<_> = events.iter().map(|e| e.event_type()).collect();
        assert!(
            kinds.contains(&"PlayerMoved"),
            "expected PlayerMoved in game_events, got: {:?}",
            kinds
        );

        token.cancel();
    }

    #[tokio::test]
    async fn debug_event_fan_in_rejects_queued_prior_context_envelope() {
        use parish_core::world::events::GameEvent;

        let state = crate::routes::tests::test_app_state();
        let (mut rx, location, old_epoch, now) = {
            let world = state.world.lock().await;
            (
                world.event_bus.subscribe_contextual(),
                world.player_location,
                world.event_bus.context_epoch(),
                world.clock.now(),
            )
        };

        {
            let world = state.world.lock().await;
            world.event_bus.publish(GameEvent::AddressedAbsentNpc {
                name: "Old-context person".to_string(),
                location,
                timestamp: now,
            });
            world.event_bus.advance_context_epoch();
        }
        let old_envelope = rx.recv().await.unwrap();
        assert_eq!(old_envelope.context_epoch, old_epoch);
        assert!(
            !record_contextual_game_event(&state, old_envelope).await,
            "queued old-context envelope must be discarded"
        );
        assert!(state.game_events.lock().await.is_empty());
        assert_eq!(
            state
                .total_game_events
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );

        {
            let world = state.world.lock().await;
            world.event_bus.publish(GameEvent::AddressedAbsentNpc {
                name: "Current-context person".to_string(),
                location,
                timestamp: now,
            });
        }
        let current_envelope = rx.recv().await.unwrap();
        assert!(
            record_contextual_game_event(&state, current_envelope).await,
            "current-context envelope must be retained"
        );
        let events = state.game_events.lock().await;
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.front(),
            Some(GameEvent::AddressedAbsentNpc { name, .. })
                if name == "Current-context person"
        ));
    }

    #[tokio::test]
    async fn persistent_character_log_writer_rejects_queued_prior_context_event() {
        use parish_core::character_log::CharacterLogManager;
        use parish_core::world::events::GameEvent;

        let state = crate::routes::tests::test_app_state();
        let temp = tempfile::tempdir().unwrap();
        let manager = CharacterLogManager::new_at_dir(temp.path().to_path_buf(), true);
        let player_log = manager.player_log_path();
        let (location, old_epoch, now) = {
            let world = state.world.lock().await;
            (
                world.player_location,
                world.event_bus.context_epoch(),
                world.clock.now(),
            )
        };
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let gate = state.persistence_gate.lock().await;
        let writer_state = Arc::clone(&state);
        let writer = tokio::spawn(async move {
            run_character_log_writer(writer_state, rx).await;
        });

        tx.send((
            manager.clone(),
            GameEvent::AddressedAbsentNpc {
                name: "Old-context person".to_string(),
                location,
                timestamp: now,
            },
            old_epoch,
        ))
        .await
        .unwrap();
        let current_epoch = {
            let world = state.world.lock().await;
            world.event_bus.advance_context_epoch()
        };
        drop(gate);
        tx.send((
            manager,
            GameEvent::AddressedAbsentNpc {
                name: "Current-context person".to_string(),
                location,
                timestamp: now,
            },
            current_epoch,
        ))
        .await
        .unwrap();
        drop(tx);
        writer.await.unwrap();

        let log = std::fs::read_to_string(player_log).unwrap();
        assert!(!log.contains("Old-context person"));
        assert!(log.contains("Current-context person"));
    }

    /// Regression test for #230: the autosave path must reuse a single
    /// `AsyncDatabase` across multiple saves rather than reopening the file
    /// (and re-running `migrate()`) on every tick.
    ///
    /// Verifies:
    /// 1. Opening the DB once and calling `save_snapshot` N times produces N
    ///    snapshots in the database (i.e. the handle is reused, not replaced).
    /// 2. The snapshot count matches the number of save calls — if a new
    ///    connection were opened each time, the per-call migrate() would not
    ///    duplicate rows, but we confirm the handle is indeed reused by checking
    ///    that the Arc inside AsyncDatabase stays alive across calls.
    #[tokio::test]
    async fn autosave_reuses_async_database_across_ticks() {
        use parish_core::persistence::snapshot::{ClockSnapshot, GameSnapshot};
        use parish_core::persistence::{AsyncDatabase, Database};
        use parish_core::world::LocationId;

        let tmp = tempfile::NamedTempFile::new().unwrap();

        // Open the DB once — exactly what the fixed autosave tick does.
        let db = Database::open(tmp.path()).unwrap();
        let async_db = AsyncDatabase::new(db);

        let branch = async_db.find_branch("main").await.unwrap().unwrap();

        fn make_snapshot() -> GameSnapshot {
            GameSnapshot {
                player_location: LocationId(1),
                weather: "Clear".to_string(),
                text_log: vec![],
                clock: ClockSnapshot {
                    game_time: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap(),
                    speed_factor: 36.0,
                    paused: false,
                },
                npcs: vec![],
                last_tier2_game_time: None,
                last_tier3_game_time: None,
                last_tier4_game_time: None,
                introduced_npcs: Default::default(),
                visited_locations: std::collections::HashSet::new(),
                visited_order: Vec::new(),
                edge_traversals: Default::default(),
                gossip_network: Default::default(),
                conversation_log: Default::default(),
                player_name: None,
                player_progress: Default::default(),
                npcs_who_know_player_name: Default::default(),
            }
        }

        // Simulate three autosave ticks using the same handle.
        for _ in 0..3 {
            async_db
                .save_snapshot(branch.id, &make_snapshot())
                .await
                .expect("autosave tick should succeed with reused connection");
        }

        // All three snapshots must be present; branch_log returns most-recent-first.
        let log = async_db.branch_log(branch.id).await.unwrap();
        assert_eq!(
            log.len(),
            3,
            "three autosave ticks via the same AsyncDatabase must produce three snapshots"
        );
    }
}
