# Dialogue finalization proof contract (#1855, #1857)

## Root cause (Five Whys)

1. A successful Gemini reply could remain as one word because the UI committed
   whatever the pacing timer had rendered when a correction arrived.
2. The correction synchronously finalized the stream entry, cleared its turn
   identity, and then tried to replace it by that cleared identity.
3. A failed length-terminated response looked like a silent turn because the
   terminal event carried only a turn ID and compact submit results projected
   only accepted canonical exchanges.
4. Both consumers inferred completion from transient token buffers because
   `stream-turn-end` was a timing marker rather than an authoritative result.
5. Candidate-token quarantine made that assumption invalid: the complete,
   validated response exists at the canonical apply seam and must cross the
   protocol explicitly.

The root fix makes the terminal event carry one typed disposition: either the
complete post-validation response and stable message identity, or a failure
with safe retry guidance and no partial text.

## Acceptance criteria

- A successful provider stream commits the full validated final dialogue in
  the browser, synchronous command drain, headless renderer, and compact turn
  projection; a paced first-word prefix is never the retained result.
- `length`, missing-terminal, failed, timed-out, and closed provider turns do
  not publish candidate text or a canonical `DialogueOccurred` exchange.
- A locally initiated failed dialogue renders a system error with a clear
  retry action and returns the same safe failure in compact submit results.
- Autonomous failures remain silent, and successful arrival-reaction token
  streams retain their existing token-only completion behavior.
- Multiple NPC replies remain serialized: an authoritative result for a
  parked speaker does not bypass the active speaker.
- Successful finalization retains the placeholder's stable message ID and
  clears streaming state, so the completed NPC bubble remains a valid reaction
  target under the directional-reaction contract landed in #1876.
- Reconnect/finalization races locate dialogue by stable message identity;
  stale timers cannot append after terminal success or failure.
- Same-session reconnect retains the canonical buffered batch, while a late
  terminal whose pending turn and stable message identity were both cleared by
  session/context replacement is ignored and cannot resurrect old dialogue.
- The composer retains focus semantics and `aria-busy` clears on terminal
  chain completion. Desktop and 390×844 layouts keep the final/error content
  inside the chat viewport.

## Prevention

No `AGENTS.md` amendment is needed. Rules 10, 31, and 37 already require a
lowest-seam escaped-bug regression, preservation of live dialogue signals, and
rejection of non-success model termination. The prevention gap was mechanical:
the shared terminal protocol lacked a typed final-result oracle and tests did
not cover the first-word/correction race or a failed turn with no exchange.
