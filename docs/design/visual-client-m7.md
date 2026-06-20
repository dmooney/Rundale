# Visual Client M7

This milestone adds a small status layer to the standalone visual client. The
client already renders scenes and supports interaction; this step makes its
runtime state legible by showing whether it is loading, ready, empty, sending, or
in an error state.

## Affected Subsystems

- `parish/apps/visual`: adds a status helper module, tests, DOM status line,
  control disabled-state wiring, and status styling.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Interaction Model

The status line is displayed beneath the scene subtitle:

- `Loading scene` while refreshing scene-state;
- `Scene ready` when `/api/scene-state` returns a scene;
- `No scene available` when scene-state returns `null`;
- `Sending command` while a command is being submitted;
- `Connection error` when fetch fails or returns an error.

During refresh or command submission, connect, refresh, command, shortcut, and
quick-action buttons are disabled. The controls re-enable when the operation
finishes or recovers from an error.

## Data Model

No backend data model changes are required. The visual client adds a pure
`client-status` helper for stable labels and control-state decisions.

## Observable Signal

The harness signal remains backend scene-state availability for Crossroads and
Darcy's Pub. The browser signal is that ready, empty, and error states are
visible and the recovered ready state has enabled controls.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
