# Boundary/Roof Retry - Cycle AI

Cycle AI reruns the third-topology Kilteevan crop after Cycle AH showed two
failures:

- likely administrative/survey boundaries became physical stone walls,
- the main building gained a chimney-like roof stack.

AI tests whether prompt/reference changes alone can fix those failures.

## Variants

| Variant | Output                                                               | Prompt                                                                     | Report                                                                     | Reference stack                                         | Result                        |
| ------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------- | ----------------------------- |
| AI-A    | `pipeline-experiments/idea-ai-a-kilteevan-boundary-roof-direct.png`  | `pipeline-experiments/idea-ai-a-kilteevan-boundary-roof-direct.prompt.md`  | `pipeline-experiments/idea-ai-a-kilteevan-boundary-roof-direct.report.md`  | Full notebook scene plus cleaned style crops            | Roof pass, boundary fail      |
| AI-B    | `pipeline-experiments/idea-ai-b-kilteevan-cleanrefs-only-direct.png` | `pipeline-experiments/idea-ai-b-kilteevan-cleanrefs-only-direct.prompt.md` | `pipeline-experiments/idea-ai-b-kilteevan-cleanrefs-only-direct.report.md` | Cleaned style crops only, no full-scene style reference | Roof/door pass, boundary fail |

## What Improved

Both AI variants fixed the roof protrusion problem. No obvious chimneys, smoke,
vents, ridge stacks, roof posts, or chimney-like roof nubs remain.

Both variants also keep the major third-crop topology readable:

- broad lower lane,
- central road-front building group,
- separate upper center-left compound,
- center-right planted enclosure,
- northeastern tree/scrub mass,
- multiple central buildings rather than one consolidated farmhouse.

AI-B is a useful signal that the cleaned style crops alone can carry a
reasonable notebook-like plate without the full notebook UI scene as a style
reference. It loses some looseness, but it also avoids the chimney failure.

## What Still Fails

Both variants fail the strict administrative-boundary no-trace requirement. The
stronger text prompt says ambiguous dotted/pecked/survey lines must leave no
continuous physical trace, but the model still converts too much linework into
continuous stone walls, stone rows, roads, or traceable boundary courses.

This suggests prompt wording alone is not enough for reliable batch generation
when the raw map contains prominent dotted/pecked boundaries. The model appears
to reason "line on map equals boundary object" unless the line is removed or
visually de-emphasized before the render stage.

## Recommendation

The next production-shaped experiment should preprocess the source/control
input rather than continue only prompt rewrites:

- Produce a cleaned map/control crop that suppresses likely non-physical
  dotted/pecked administrative boundaries before image generation.
- Preserve roads, building marks, solid enclosure lines, tree/scrub symbols,
  and planted-enclosure texture.
- Feed both the original crop and the cleaned no-admin crop to imagegen, with
  the cleaned crop as "physical linework authority" and the original crop as
  source evidence.
- Keep AI-B's no-chimney/door language and consider omitting the full notebook
  scene if semantic leakage remains a risk.

Do not promote AI-A or AI-B as clean visual targets. Treat them as evidence
that roof/door/style issues are increasingly controllable, while
administrative-boundary suppression needs an upstream control artifact.
