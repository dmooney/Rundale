Verdict: sufficient
Technical debt: clear

All 10 TODO.md items (TD-008 through TD-017) were resolved in this PR.

- Dead code was deleted (not commented out): GameSpeed::factor_with_config, ConversationLog::last_speaker_at, GossipNetwork::recent.
- Serialize/Deserialize derives were added to five types (Festival, TimeOfDay, Weather, GameSpeed, SpeedConfig) for consistency.
- GossipNetwork eviction was refactored from O(n log n) sort+drain to O(1) VecDeque+pop_front.
- Six new tests were added: four Display round-trip tests, two EventBus lag/overflow tests, and one GameClock frozen-speed test.
- Stale documentation was corrected: broken intra-doc link removed, README module list updated.
- The TODO.md was updated with resolution details and a discovery log entry.

No new technical debt was introduced. Workspace callers (parish-core, parish-cli) compile without modification.
