# Gameplay proof scripts

These scripts are human- or agent-read evidence captured for a particular
change. They exercise the game, but their output is not machine-asserted and
they are not a regression gate.

Permanent deterministic coverage belongs in `../scenarios/` as a versioned
YAML scenario with explicit assertions. The remaining legacy regression
scripts in `../fixtures/` are kept temporarily for compatibility and are the
only plain-text scripts swept by `just game-test-all`.
