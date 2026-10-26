# Linux host

`rundale-linux-host` plays the iPhone app's game on Linux and prints the
screen as Markdown frames (#2273, step 5 of #2268). It runs the app's own
session logic, `RundaleSession`'s controllers and presenter, against the
real engine, linked from `liblimerick_mobile_ffi.a` built for Linux, on
`mods/rundale` as the app ships it. Frames come from the presenter's state
only; the host decides no game or session behaviour.

Frames are not app UI, simulator, or device evidence. They show nothing about
layout, the keyboard, scrolling, or accessibility. See
[build-test.md](../../docs/agent/build-test.md#linux-host-frames).

## Run it

```sh
just linux-host                       # mock Endpoint, new game, lines from stdin
just linux-host --script my-run.txt   # lines from a file
just linux-host --resume              # continue the last game
just linux-host --inference fixture   # RundaleKit's fixture session, no engine
just linux-host --inference sse --sse-fixture mobile/endpoint/fixtures/dialogue-v1.sse
just linux-host-test                  # golden-frame tests
```

`run.sh` builds the engine's static library with Cargo, then the host with
SwiftPM, and passes `--mod-dir mods/rundale` and
`--save-dir tools/linux-host/.build/save` unless you give your own.

Inference modes:

- `mock` (the default): the engine, with the scripted mock Endpoint the phase
  UI suites play against (`Phase2MockEndpointTransport`). Lines can ask it for
  a failure: "busy once", "offline once", "disconnect once", "fail", or a
  slower stream with "slow".
- `sse`: the engine; dialogue replays a recorded Endpoint stream under the
  call's identities, and intent goes to the mock.
- `fixture`: `RundaleFixtureController` over RundaleKit's fixture adapter.
- Live play against `limerick-prod` is #2274.

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
  not printed, so frames do not depend on timing.
- `clarification`, `committed`, `failed`, `stopped`: the turn settled, with
  nothing in flight and no change for 0.4 s.
- `timed out`: the turn ran past 60 s; the host exits 1.

## Golden tests

`Tests/RundaleLinuxHostTests/Scenarios/*.txt` are host scripts; the first
`# inference:` line picks the mode. Each test plays one and compares its
frames with `Goldens/<name>.md`. After an intended change, regenerate and
review the diff:

```sh
RUNDALE_UPDATE_GOLDENS=1 just linux-host-test
git diff tools/linux-host/Tests
```

CI's `Linux host golden frames` job runs them on Linux.
