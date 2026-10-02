# Phase demonstrations

Added 2026-09-07 at the user's request: give the user a demo at the conclusion
of every phase. This is part of delivery alongside the existing
[product requirements](product-technical-spec.md), not a substitute for them.

## Format

Use the running application and a reproducible starting fixture or save. Show
the relevant player interactions, explain what has changed, and include the
phase's important failure/recovery cases. Make the demo available to the user
with the running build and concise screenshots or a recording when useful.
Identify the device or simulator, build revision or working-tree state,
verification results, and any pending gates. Do not present a simulator demo
as physical-iPhone validation or a fixture as live gameplay.

### Recordings

A recording is for a person to watch, not for a test runner to pass. Pace it
so each state can be read at normal speed:

- Hold at least 3 seconds on the app after launch, before the first input.
- After each result settles (a reply finishes streaming, a scene changes, a
  Stop or error appears), hold long enough to read it: at least 3 seconds, and
  about 1 second per line of new text.
- Hold at least 5 seconds on the final state. Do not end on, or cut to, the
  test runner, the home screen, or the app closing.
- Do not speed up footage. Trim dead time between steps instead.

A UI-test run (XCUITest) moves faster than a person can follow and tears the
app down as soon as it passes. Add explicit holds in a demo run, or edit the
footage to freeze on each settled state, before sharing it.

Publish a PR's recording on its public evidence page, not as a private link.
Put the page's files (an `index.html`, the video as H.264 `.mp4`, and
optionally a short `.gif`) in one directory and run
`bash limerick/scripts/publish-pr-page.sh <pr-number> <directory>`. It
publishes to `https://dmooney.github.io/rundale-pages/pr/<pr-number>/`. Link
that page from the PR body, and embed the `.gif` from it so the recording plays
inline in the PR.

An implementation preview may be demonstrated before acceptance is complete.
Label it as an interim demo and keep the phase open until the full Exit
Criteria and Definition of Done have been met.

## Demonstration sequence

| Phase                            | Demonstrate                                                                                                                                                                                                                                                                       |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 — Native interaction prototype | Fixture-only header/transcript/composer; multiline input; incremental output and Stop; editable history; @ and / completion; clarification and retry fixtures; reading history while output arrives; newest-content return; draft/session restoration; light/dark and large text. |
| 2 — Embedded Rust slice          | Real local game on the canonical tiny world; offline /look; real incremental Limerick Endpoint response; Stop/retry; completed action and interrupted request recovery after relaunch.                                                                                            |
| 3 — Tiny world navigation        | All three locations and NPCs against the canonical sheet; deterministic offline and natural-language travel; presence, a scheduled movement, clarification, and save/resume consistency.                                                                                          |
| 4 — Mobile reliability           | The existing game through keyboard changes, app switching, connectivity loss/recovery, interrupted inference, long history, accessibility settings, and relaunch; include the required physical-device session evidence.                                                          |
| 5 — Living-world proof           | Reproducible memory, gossip before/after propagation, task progression, weather behavior, and scheduled movement; inspect authoritative state and repeat after save/resume.                                                                                                       |
| 6 — Controlled expansion         | For every accepted increment, demonstrate its specific player benefit and acceptance cases, then show that the canonical tiny-world and affected reliability interactions still work.                                                                                             |

Keep demonstrations within their phase scope. Record any observed defect and
fix or clearly report it rather than presenting an edited success-only path as
evidence of reliability.
