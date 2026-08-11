# Issue #1835 — Mobile Chat Bottom-Follow Audit

Date: 2026-08-10

## Five Whys

**Observed problem:** at 390×844, a completed NPC reply existed in the DOM but
was entirely below the visible chat viewport, which remained 140 px short of
its maximum scroll position.

1. Why? `ChatPanel` stopped changing `scrollTop` while the reply continued to
   gain rendered height.
2. Why? Its sticky-bottom effect treated only `textLog.length` growth as new
   transcript content.
3. Why? Streaming inserts one empty placeholder and then replaces that same
   entry for every revealed chunk, so the array length stays constant while
   the bubble grows.
4. Why did coverage miss this? Unit tests appended entries, while browser tests
   asserted that streamed text was in the DOM rather than inside the visible
   chat rectangle.
5. Why was DOM presence accepted? The contract modeled data cardinality, not
   rendered transcript growth or the space removed by viewport/composer resize.

**Root cause:** bottom-follow was coupled to array-length growth instead of
rendered transcript revisions and available chat height.

**Prevention:** no new `AGENTS.md` rule is needed. Rules 29 and 31 already
require live desktop/mobile interaction auditing and visible streamed dialogue.
The missing enforcement is now supplied by same-entry unit tests and browser
geometry assertions against the chat and persistent composer.

## Interaction Model

Chat remains the primary desktop and mobile play surface. The transcript owns
vertical scrolling; the composer remains persistent below it. A player at or
within 50 px of the bottom follows transcript revisions and layout resizes. A
player who deliberately scrolls farther upward keeps that reading position.
Local submission returns to live follow, but a later user scroll overrides the
pending echo. Scrolling never moves keyboard or accessibility focus.

## Interaction Inventory

| Action or change                                 | Result while following                                 | Result while reading above |
| ------------------------------------------------ | ------------------------------------------------------ | -------------------------- |
| NPC stream chunk/finalization/correction         | Latest rendered text remains above the composer        | Position is preserved      |
| Reaction or other transcript revision            | Bottom alignment is retained                           | Position is preserved      |
| Desktop/mobile viewport resize                   | Latest reply remains visible                           | Position is preserved      |
| Composer grows to multiple lines                 | Available chat area reflows and remains bottom-aligned | Position is preserved      |
| Local message submission                         | Immediately returns to live follow                     | Same                       |
| User wheel/touch/scrollbar movement after submit | Disables the delayed echo follow beyond 50 px          | Continues reading above    |

The existing `role="log"`, polite live region, message grouping, and composer
focus semantics remain unchanged.

## Findings and Resolution

| Severity | Finding                                                     | Resolution                                                                    | Evidence                                           |
| -------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------- |
| High     | Same-entry streaming grew below the mobile viewport         | Follow every `textLog` array revision after Svelte commits the DOM            | `ChatPanel.test.ts`; desktop/mobile geometry tests |
| High     | Native layout scroll events could masquerade as user intent | Unstick only after wheel, touch, scrollbar, or explicit synthetic test intent | scrolled-up stream and queued-scroll tests         |
| Medium   | Composer/viewport shrink had no follow signal               | Observe the chat flex child with `ResizeObserver` while sticky                | resize/disconnect unit tests; mobile composer E2E  |
| Medium   | Browser tests checked DOM containment, not visibility       | Assert reply/chat/composer rectangles and maximum scroll delta                | `interactions.spec.ts`                             |

## Visual and Live Proof

The Playwright visual contract captures deterministic 1440×900 desktop and
390×844 mobile states after an overflowing transcript and completed NPC reply:

- `gui-desktop-latest-reply.png`
- `gui-mobile-latest-reply.png`

The corresponding semantic run streams the reply through the production page
controller and stream manager, verifies its whole short bubble is within the
chat viewport above the composer, then exercises mobile viewport contraction,
multi-line composer growth, and a user scroll during streaming. Test and gate
results are recorded in the PR proof bundle for #1835.
