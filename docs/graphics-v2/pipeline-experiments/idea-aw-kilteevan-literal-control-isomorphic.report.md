# Cycle AW Literal Control Isomorphic Audit

Generated with the built-in `image_gen` tool from the exact prompt in `idea-aw-kilteevan-literal-control-isomorphic.prompt.md`.

Output file: `docs/graphics-v2/pipeline-experiments/idea-aw-kilteevan-literal-control-isomorphic.png`

## Audit

- Format/style: Pass. The result is a 16:9 illustrated low 3/4 orthographic plate with strong parish-notebook ink, watercolor, muddy roads, readable facades, and no UI.
- Negative leakage: Mostly pass. I see no people, animals, visible text, water, church/shop/bridge objects, smoke, or obvious chimneys.
- Doors/thresholds: Pass visually. The visible buildings have dark doorway reads and small thresholds/yard connections.
- Walkability: Partial pass. The main roads are broad and continuous, but some gates/walls narrow and formalize yards more than requested.
- Source/topology fidelity: Fail/weak. The model regularized the scene into a picturesque centered crossroads and appears to invent or over-emphasize several buildings/compound relationships beyond the map-supported marks.
- Open-field/admin-boundary handling: Fail/weak. Open fields remain visible, but continuous stone-wall chains and road-border walls are much stronger and more complete than the prompt allowed; this risks materializing parcel/admin/survey linework or control linework as physical boundaries.
- Deterministic-control interpretation: Partial. The control colors are not copied literally, but the final image beautifies and completes the control into a scenic village layout rather than preserving the awkward tight crop.

## Verdict

Useful style/camera sample, but not a clean success for the Cycle AW recipe. Main failures are scenic-crossroads regularization, overbuilt continuous walls, and weak fidelity to the tight crop's awkward map-derived topology.
