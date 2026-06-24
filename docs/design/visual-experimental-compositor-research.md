# visual-experimental-compositor-research

The user explicitly invited ideas beyond a normal game-dev approach. The
direction worth exploring is not "replace the compositor with AI art." It is:
use research-flavored generation to propose raster atoms and placements, then
force everything back through the Parish `SceneState.layers` contract so the
game remains inspectable, clickable, and testable.

## Useful Research Threads

- **Example-based texture synthesis:** Efros and Leung's non-parametric
  texture synthesis grows new imagery from a sample by matching local
  neighborhoods and preserving local structure. For Rundale, this suggests a
  road/wall/foliage atom generator seeded from approved pixel-art exemplars,
  not freeform whole-scene generation.
  <https://www2.eecs.berkeley.edu/Research/Projects/CS/vision/papers/efros-iccv99.pdf>
- **Wave Function Collapse / model synthesis:** WFC generates outputs whose
  local pixel/tile patterns come from the input sample, and supports
  constraints. That maps well to "generate plausible wall/road/foliage
  placement blueprints, then render them as small PNG layer instances."
  <https://github.com/mxgmn/WaveFunctionCollapse>
  <https://paulmerrell.org/model-synthesis/>
- **Image analogies / style transfer by correspondence:** The useful idea is
  not full neural style transfer, but paired examples: "rough mask/semantic
  plan" to "approved pixel-art atom sheet." This could let us generate new
  atom sheets for a bridge, hedgerow, puddles, or cottage props from clean
  masks while keeping a consistent art direction.

## Experiments To Try

1. **Atom Sheet From Reference**
   - Input: the approved Kilteevan/Crossroads reference style, plus masks for
     wall segment, bramble clump, puddle, cart, sign, door, smoke.
   - Output: a transparent PNG sprite sheet of 12-24 atoms.
   - Gate: every exported atom must be under a size threshold, have meaningful
     alpha bounds, and be placeable independently in `SceneState.layers`.

2. **Compositor Proposal Tool**
   - Input: a scene slug and a set of candidate atoms.
   - Output: proposed layer ids, `x/y`, `z`, `scale`, `flip`, and opacity.
   - Gate: the tool may suggest, but tests own the contract: no SVGs, no
     object full-frame crops, repeated asset usage required, hotspots remain
     independent of display labels.

3. **Depth And Occlusion Masks**
   - Generate or hand-author a coarse depth map for each scene.
   - Use it to sort NPCs and props without manually guessing every z-index.
   - Gate: NPC feet must land on valid walk/slot bands; foreground objects can
     occlude them intentionally.

4. **Constraint-Based Road/Wall Fill**
   - Treat roads, walls, hedges, and brambles as constrained texture regions.
   - Use WFC-like local-pattern rules to place small atoms along masks.
   - Gate: the final output is still ordinary scene JSON plus PNG atoms, not a
     runtime black box.

5. **Asset Critic Loop**
   - Run a screenshot judge that flags repeated-stamp artifacts, broken
     perspective, scale drift, or UI/gameplay occlusion.
   - Feed failures back into atom placement or generation prompts.
   - Gate: screenshots at desktop/mobile remain the acceptance artifact.

## Why This Fits Parish

Parish already has the right boundary: `SceneState.layers` is a transparent
contract. The experimental layer can live outside the runtime as authoring
tools that emit assets and scene JSON. If a generated idea is bad, it fails
tests or visual proof; if it is good, it becomes deterministic data the client
can render like any other sprite composition.
