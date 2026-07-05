# Paired Repair - Cycle Z

Cycle Z performs targeted repairs on the current X/Y pair rather than changing
the pipeline:

- Beechwood Z starts from Beechwood X and tries to clarify the one ambiguous
  right-side exterior doorway while roughening garden/wall regularity.
- Grove Z starts from Grove Y and tries to lower the camera slightly while
  preserving Grove's separate-building yard topology.

The goal is to see whether small repair/refinement passes can improve the best
paired outputs without losing the topology wins that made X/Y useful.

## Outputs

| Site        | Output                                                              | Prompt                                                                    | Report                                                                    | Result                         |
| ----------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------ |
| Beechwood Z | `pipeline-experiments/idea-z-beechwood-x-door-roughness-refine.png` | `pipeline-experiments/idea-z-beechwood-x-door-roughness-refine.prompt.md` | `pipeline-experiments/idea-z-beechwood-x-door-roughness-refine.report.md` | Conservative repair pass       |
| Grove Z     | `pipeline-experiments/idea-z-grove-y-lower-camera-refine.png`       | `pipeline-experiments/idea-z-grove-y-lower-camera-refine.prompt.md`       | `pipeline-experiments/idea-z-grove-y-lower-camera-refine.report.md`       | Conservative camera/style pass |

## Audit Questions

- Did the repair preserve the source topology, or did it make a prettier but
  less map-faithful plate?
- Did the Beechwood right-side opening become a clear doorway/threshold?
- Did Grove become less roof-dominant without copying Beechwood's connected
  compound arrangement?
- Did garden rows and stone walls become rougher without moving boundaries?
- Does the repaired pair look more consistent with the original illustrated
  parish notebook sample than X/Y?

## Result

Both Z passes are conservative improvements rather than new endpoints.

Beechwood Z is the cleaner repair: it keeps the Cycle X compound connected,
preserves the close crop, and clarifies the right-side exterior side opening as
a proper dark doorway/threshold. It also roughens planted rows and stone walls
without moving their boundaries. Prefer Beechwood Z over Beechwood X for the
current Beechwood candidate.

Grove Z preserves the separate-building yard topology from Grove Y and becomes
slightly lower/more facade-readable. It does not copy Beechwood's connected
compound. It remains a bit more roof-visible than Beechwood Z, but it is a
better style/camera companion than Grove Y. Prefer Grove Z over Grove Y for the
current Grove candidate.

## Recommendation

The leading candidate pair is now:

- Beechwood Z for connected-compound topology.
- Grove Z for separate-building yard topology.

Use the same method for the next unrelated crop: choose a small local topology
crop first, generate an oblique pitch cue, render with topology locked, then
allow at most one conservative repair pass. Avoid iterative polishing beyond
that unless a specific audit failure is being fixed; repeated refinement risks
over-anchoring or drifting away from the map.
