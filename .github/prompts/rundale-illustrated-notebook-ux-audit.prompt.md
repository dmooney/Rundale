# Rundale Illustrated Notebook UX Audit

Audit the live illustrated notebook as a player completing ordinary tasks, not
as a test author enumerating controls.

1. Run the actual UI at desktop (1440×900) and mobile (390×844) sizes.
2. Capture the untouched first viewport. Identify the primary action,
   navigation, current status/feedback, and a safe visible way to leave or back
   out.
3. Activate every visible control with a pointer. Record its player-facing
   result, whether that result matches the control's visual promise, and the
   obvious return path.
4. For notebook tabs, verify the named section appears on the sewn notebook
   page. For Places, verify a place directory/record. For Map, verify geographic
   orientation/navigation. These must be visibly distinct destinations.
5. For each deliberate overlay, verify notebook visual language, task context,
   focus entry, Escape/Close/backdrop behavior where applicable, internal
   scroll or map zoom/pan, and focus restoration.
6. Before and after every destination, verify the scene, selected person,
   command draft, scroll/zoom state where relevant, and canvas bounds are
   preserved unless the action intentionally changes them.
7. Repeat the pointer walkthrough at mobile size and capture every changed
   destination.
8. Add behavioral and Playwright assertions for semantic results and state
   preservation. “Dialog exists” and “canvas did not resize” are supporting
   checks, not the success contract.
9. Finish with a findings table: expectation, evidence, resolution, and any
   deliberate deferral with an owner/issue.

Do not pass the audit with tooltips, modal restyling, test-only selectors, or
undocumented text commands. The interface must read as one understandable game.
