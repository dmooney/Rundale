Evidence type: Playwright test run (45/50 pass, all 6 new tests green)

Verdict: sufficient

Technical debt: clear

The new tests verify data flows end-to-end through the mock IPC layer into
debug panel tabs (Overview clock, Weather weather, Gossip empty, Conversations
empty, World stats) and the save picker branch load flow. The SetupOverlay
mock regression fix makes the entire E2E suite more reliable (2 previously
flaky tests now pass consistently).
