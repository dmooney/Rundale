# Rundale CLI

`rundale-cli` plays the iPhone app's game from the command line and prints the
screen as Markdown frames (#2273, step 5 of #2268). It runs the app's own
session logic, `RundaleSession`'s controllers and presenter, against the
real engine, linked from `liblimerick_mobile_ffi.a` built for the current platform, on
`mods/rundale` as the app ships it. Frames come from the presenter's state
only; the host decides no game or session behaviour.

Frames are not app UI, simulator, or device evidence. They show nothing about
layout, the keyboard, scrolling, or accessibility. See
[build-test.md](../../docs/agent/build-test.md#rundale-cli-frames).

## Run it

```sh
just rundale-cli                       # mock Endpoint, new game, lines from stdin
just rundale-cli --script my-run.txt   # lines from a file
just rundale-cli --resume              # continue the last game
just rundale-cli --inference fixture   # RundaleKit's fixture session, no engine
just rundale-cli --inference sse --sse-fixture mobile/endpoint/fixtures/dialogue-v1.sse
just rundale-cli-test                  # golden-frame tests
```

`run.sh` builds the engine's static library with Cargo, then the host with
SwiftPM, and passes `--mod-dir mods/rundale` and
`--save-dir tools/rundale-cli/.build/save` unless you give your own.

Inference modes:

- `mock` (the default): the engine, with the scripted mock Endpoint the phase
  UI suites play against (`Phase2MockEndpointTransport`). Lines can ask it for
  a failure: "busy once", "offline once", "disconnect once", "fail", or a
  slower stream with "slow".
- `sse`: the engine; dialogue replays a recorded Endpoint stream under the
  call's identities, and intent goes to the mock.
- `fixture`: `RundaleFixtureController` over RundaleKit's fixture adapter.
- `live`: the real Endpoint client with portable Firebase Auth and App Check
  REST credentials. See [terminal MCP setup](../../docs/agent/runtime-driving-reference.md#portable-app-session-through-mcp).

The host also runs in a macOS terminal. Frames identify the actual platform.
`--protocol` starts the private managed-child interface used by the existing
`limerick-mcp --backend session-host`; it does not open a game until an explicit
new/resume action. This mode supports asynchronous inspection and Stop.

## Terminal formatting

Frames use [Swiftdansi](https://github.com/steipete/Swiftdansi) for ANSI styling
and wrapping when stdout is a terminal. Piped output stays raw Markdown.
Use `--format markdown` to force raw frames, or `--format ansi` to force
styling even through a pipe. The default is `--format auto`.

In auto mode, a non-empty `NO_COLOR` disables ANSI styles while retaining rendered layout;
`TERM=dumb` keeps raw Markdown. Explicit `--format ansi` overrides both.
The private `--protocol` interface always returns its original JSON and Markdown.

## Input

A line is typed into the composer and sent. Host commands stand in for taps:

| Line             | Does                                                    |
| ---------------- | ------------------------------------------------------- |
| `N`, `:choose N` | Picks clarification choice N.                           |
| `:stream TEXT`   | Sends TEXT and returns once the reply starts streaming. |
| `:stop`          | Taps Stop.                                              |
| `:retry`         | Taps Retry.                                             |
| `:restart`       | Saves, closes the engine, and opens the save again.     |
| `:frame`         | Prints the current frame.                               |
| `:quit`          | Saves and exits.                                        |
| `:help`          | Lists these commands.                                   |

Lines starting with `#` are comments. Each command waits until the turn
settles before the next line runs, except `:stream`.

## Frames

A frame is the status header, the transcript, the composer (draft, then Send
or Stop, and Retry when offered) and any pending choices, numbered. Its
heading names why it was printed:

- `new game`, `restored`: the game opened.
- `submitted`: right after Send.
- `streaming`: a reply's provisional text has held still for half a second.
  A delta replaced sooner, such as the last one before the final reply, is
  not printed. Frames depend on the mock's pacing (2 s between deltas, 1 s for
  the recorded stream), not on races within it.
- `clarification`, `committed`, `failed`, `stopped`: the turn settled, with
  nothing in flight and no change for 0.4 s.
- `timed out`: the turn ran past 60 s; the host exits 1.

## Golden tests

`Tests/RundaleCLITests/Scenarios/*.txt` are host scripts; the first
`# inference:` line picks the mode. Each test plays one and compares its
frames with `Goldens/<name>.md`; a mismatch leaves the new frames in
`.build/golden-actual/`, which CI uploads. After an intended change, regenerate and
review the diff:

```sh
RUNDALE_UPDATE_GOLDENS=1 just rundale-cli-test
git diff tools/rundale-cli/Tests
```

CI's `Rundale CLI tests (Linux)` job runs them on Linux.
