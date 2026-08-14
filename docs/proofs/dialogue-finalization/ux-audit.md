# Dialogue finalization interaction audit

## Interaction model

The illustrated notebook chat remains the primary work surface. The transcript
is the persistent reading surface and the composer is the next-action surface;
dialogue completion does not open an overlay, navigate away, or move focus.
Progressive text is presentation only. The terminal event replaces it in place
with canonical dialogue, or removes it and adds a visually distinct system
error that tells the player to try again.

## Inventory

| Cue or control | Player expectation | Result | Context and exit |
| --- | --- | --- | --- |
| Growing NPC bubble | The current reply is still arriving | Terminal success leaves the complete validated reply in the same transcript position | Notebook, scroll position, message identity, and reaction target remain continuous |
| Red system error | The attempted reply failed and was not accepted | No partial NPC text remains; “Please try again” points back to the composer | No overlay or dead end; the composer becomes available after `stream-end` |
| Composer | Enter another action/dialogue after completion | Existing submit/focus behavior is unchanged and `aria-busy` becomes `false` | Player stays on the same notebook page |

## Findings and resolution

| Severity | Finding | Resolution |
| --- | --- | --- |
| High | A correction could clear turn identity before replacement and permanently retain only the paced first word | Terminal success now commits full canonical text atomically by turn/message identity |
| High | A rejected provider completion could produce an unexplained empty compact turn and no visible recovery | Terminal failure discards partials, renders safe retry guidance, and populates the compact result’s optional `error` |
| Medium | Immediate finalization could bypass the existing one-speaker-at-a-time reveal for a parked NPC | Parked authoritative text remains buffered until promotion |
| Medium | Replacing a streamed entry could drop its reaction identity or leave it permanently “streaming” and non-reactable | Terminal completion preserves the placeholder ID, clears streaming metadata, and browser coverage opens the reaction picker on the finalized bubble |

The change adds no new control or destination, so the notebook’s persistent
navigation and overlay model are unchanged. Browser evidence covers both the
desktop surface and a 390×844 mobile viewport.
