# Parish Development Journal

> [Docs Index](index.md)

Notes, observations, and recommendations carried between sessions.

---

## 2026-03-18 — Phase 1 Complete

Phase 1 (Core Loop) is fully done. All roadmap items checked off. The game boots, renders a TUI, accepts natural language input, sends it through the Ollama inference pipeline, and renders NPC responses.

### Changes this session

- **Esc clears input** instead of quitting. `/quit` is now the only exit path.
- **Ollama auto-start/stop**: `OllamaProcess` checks if Ollama is reachable on startup, spawns `ollama serve` if not, and kills it on exit (with `Drop` safety net).

### Recommendations for Phase 2 (World Graph)

1. **Start with movement.** Hand-author 10-15 locations as JSON and wire up "go to X" commands. Movement + time-passing during travel is the single biggest gameplay unlock.
2. **Defer OSM extraction.** Hand-authored locations are fine for prototyping. The OSM tool is a nice-to-have and can come later without blocking anything.
3. **Dynamic location descriptions** layer well on the existing inference pipeline — enrich templates with time-of-day, weather, and NPC presence.
4. **En-route encounters** can be simple at first (random NPC sighting, weather change) and expanded later.

### Recommendations for Phase 3+

5. **Decide player identity before Phase 3.** NPC behavior (how they greet, what they share, suspicion level) depends heavily on whether the player is a known local, a newcomer, or an observer. See `docs/plans/open-questions.md`.
6. **Pull forward a minimal autosave.** Even a simple save-on-quit before the full Phase 4 branching system would reduce friction during testing.
7. **Nail down the verb set.** Minimal (look/go/talk) vs. moderate (also take/use/give) vs. expansive affects parser complexity and NPC prompt design.

### Technical notes

- **Ctrl+C handling**: Currently Ctrl+C will leave the terminal in raw mode and won't stop a Parish-started Ollama. A graceful signal handler (tokio::signal) should be added.
- **Test coverage**: 52 tests passing, 1 ignored (live Ollama test). Coverage should be verified with `cargo tarpaulin` before Phase 2 starts.
- **Inference latency**: No metrics yet. Consider adding request timing before scaling to multiple NPCs in Phase 3.
