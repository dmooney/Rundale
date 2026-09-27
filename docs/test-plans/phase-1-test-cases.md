# Rundale Phase 1 Test Cases

> Imported on: 2026-09-07
> Source modified (UTC): 2026-09-05 00:49:08.755 UTC
> Google source: [Google Doc](https://docs.google.com/document/d/1heQa_bGCw00Rcyn1joz8w9PgPpkfwpv5eBfvEpcYYKw/edit)
>
> Note: This test plan is source-derived planning material and is not execution evidence.

These cases validate Phase 1 of the mobile-first pure-text player experience. They focus on the transcript/composer interaction model, streaming behavior, command affordances, accessibility, and the intentionally narrow UI scope defined by the product specification.

## 1. Launch state

- Launch the app fresh.
- Verify the primary play screen appears immediately.
- Verify only the header, transcript, and composer are persistently visible.

## 2. Basic command submission

- Type “look around”.
- Submit.
- Verify the player command appears immediately in the transcript.
- Verify the composer clears or returns to the expected ready state.

## 3. Multiline composer

- Enter a 3–4 line command.
- Verify the composer expands without covering the transcript excessively.
- Submit and verify formatting is preserved sensibly.

## 4. Keyboard behavior

- Open the keyboard, dismiss it, and reopen it.
- Move between text entry and transcript scrolling.
- Verify the composer remains visible and correctly positioned.
- Verify no transcript content becomes unreachable.

## 5. Streaming response

- Trigger a fixture that streams for 10–20 seconds.
- Verify text appears incrementally.
- Verify the layout does not jump or flicker excessively.

## 6. Stop

- Start a long streaming fixture.
- Stop it partway through.
- Verify streaming ends promptly.
- Verify partial output remains coherent.
- Verify the composer becomes usable again.

## 7. Auto-follow while at bottom

- Stay at the newest transcript position.
- Start streaming.
- Verify newest content remains naturally visible as output arrives.

## 8. Read history while streaming

- Start streaming.
- Scroll several screens upward.
- Verify incoming text does not force the view back to the bottom.

## 9. New-content indicator

- While scrolled up, allow more output to arrive.
- Verify the UI signals that newer content exists.
- Activate that affordance.
- Verify it returns cleanly to the newest content.

## 10. Long transcript

- Load a fixture with hundreds or thousands of transcript events.
- Scroll rapidly up and down.
- Verify interaction remains responsive with no obvious rendering stalls.

## 11. Transcript type differentiation

- Show narration, NPC dialogue, player commands, deterministic/system output, errors, and scene transitions together.
- Verify a tester can distinguish them without chat bubbles or permanent side panels.

## 12. Command history

- Submit several commands.
- Recall an earlier command.
- Edit it.
- Resubmit it.
- Verify the original transcript entry remains unchanged and the edited version becomes a new command.

## 13. @ completion

- Type “@”.
- Verify fixture NPC completions appear.
- Select one.
- Verify insertion into the composer is predictable and editable.

## 14. / completion

- Type “/”.
- Verify fixture slash-command completions appear.
- Select one and execute it.
- Verify deterministic-style output appears correctly in the transcript.

## 15. Draft preservation during interruption

- Type a substantial unsent command.
- Background the app briefly.
- Return.
- Verify the draft remains intact.

## 16. Light/dark mode

- Switch system appearance while the app is open.
- Verify transcript hierarchy, composer, focus state, and controls remain legible and usable.

## 17. Dynamic Type

- Test default, large, and accessibility text sizes.
- Verify essential controls remain reachable.
- Verify the composer remains usable.
- Verify the transcript does not become structurally broken.

## 18. VoiceOver

- Navigate through the header, transcript entries, composer, Stop control, and newest-content control.
- Verify labels are meaningful and focus order is logical.

## 19. Small-screen iPhone

- Test on the smallest supported iPhone screen class.
- Open the keyboard.
- Enter multiline text.
- Stream output.
- Verify the app remains usable rather than cramped or obstructed.

## 20. Forbidden-UI regression

- Verify no graphical map, portrait panel, tab bar, NPC sidebar, save screen, debug panel, or other legacy player UI has been introduced.
