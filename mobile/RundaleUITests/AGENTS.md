# Native UI tests

- Long fixture histories arrive asynchronously. Before measuring scrolling,
  wait for the expected final row in `app.waitForTranscriptRow`; composer
  readiness alone does not establish that the history has loaded. Keep the
  viewport visibility deadline separate from this data-readiness barrier.
- Transcript rows are combined accessibility elements. Query their
  `transcript.item.*` identifiers through `.any`, then assert their visible
  geometry rather than depending on a `staticTexts` classification.
