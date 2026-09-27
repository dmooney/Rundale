# Legacy Client and HTTP Harness Rules

This file groups rules for the existing portable Node and HTTP client-harness
surface. They apply when maintaining those systems and do not impose a legacy
client implementation on mobile feature work. If a new tool uses portable Node
automation or an HTTP harness, the original conditions below still apply;
shared engineering, persistence, inference, and evidence contracts remain
mandatory for every client.

See [engineering-rules.md](engineering-rules.md) for the complete numbered
map.

<a id="rule-25"></a>

## Rule 25 — **Launch portable Node tools without a shell:**

Cross-platform Node automation must use `shell: false` and invoke JavaScript
CLI entry points through `process.execPath`, never platform wrappers such as
`.cmd`; execute the default path in tests with spaces in filesystem paths.

<a id="rule-35"></a>

## Rule 35 — **Long-running HTTP harnesses must prove session and interaction continuity:**

Reuse one authoritative server session across more requests than the
configured admission cap, including local HTTP when production cookies are
`Secure`, and reset/reacquire dialogue after terminal interactions so every
counted sample actually reaches inference. A harness that silently creates
sessions or counts post-farewell no-op commands invalidates soak/reliability
evidence.
