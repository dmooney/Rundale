# Irish Dry-Stone Wall Reference

## Why This Exists

Graphics V2 prompts have been saying "rough stone walls," but recent outputs
still drift toward uniform rectangular masonry blocks. That is wrong for
historic Irish rural field boundaries. Use this note whenever the background
plate pipeline asks for walls, field boundaries, garden walls, or walled lanes.

## Web Reference Review

Sources reviewed:

- UNESCO, "Art of dry stone construction, knowledge and techniques":
  <https://ich.unesco.org/en/RL/art-of-dry-stone-construction-knowledge-and-techniques-02106>
- Ireland National Inventory of Intangible Cultural Heritage, "Dry Stone
  Construction":
  <https://nationalinventoryich.tcagsm.gov.ie/dry-stone-construction/>
- Teagasc, "Dry Stone Wall Building":
  <https://teagasc.ie/rural-economy/rural-development/diversification/dry-stone-wall-building/>
- Government of Ireland, "Ireland's Dry Stone Construction Receives UNESCO
  Recognition":
  <https://www.gov.ie/en/department-of-culture-communications-and-sport/press-releases/irelands-dry-stone-construction-receives-unesco-recognition/>
- RTE, "'Hugely Irish' dry stone wall construction on UNESCO list":
  <https://www.rte.ie/news/ireland/2024/1205/1484917-dry-stone-walls-unesco/>
- Dry Stone Wall Association of Ireland:
  <https://www.dswai.ie/>
- Teagasc, "Dry Stone Wall Building":
  <https://www.teagasc.ie/rural-economy/rural-development/diversification/dry-stone-wall-building/>
- Roscommon County Council, "Landscape Character Assessment of County
  Roscommon":
  <https://www.rosdevplan.ie/rccdevpdfs/final/RCC-Dev-Plan-Landscape-Character-Assessment.pdf>
- Roscommon County Council, "County Roscommon's Hedges":
  <https://www.roscommoncoco.ie/en/download-it/heritage-publications/county-roscommon-s-hedges.pdf>
- Mayo County Council, "Mayo's Hedgerows":
  <https://www.mayo.ie/getmedia/4bf3ecb4-83b4-46e5-a7ed-608bbe2ade3c/Mayo-Hedgerow-Booklet-Final.pdf>

Image review covered Aran Islands walls, Burren limestone walls, west-of-Ireland
field boundaries, and farm-wall repair examples. The consistent visual lesson:
these are mortarless, locally gathered, irregular stone constructions, not
dressed masonry.

Local web-reference images are saved under
`web-references/irish-dry-stone-walls/`:

- `irish-dry-stone-wall-reference-sheet.png` — two-image reference sheet for
  future imagegen prompts.
- `fahee-north-dry-stone-wall-2015.jpg` — close texture reference for slabby,
  gapped, interlocked wall construction.
- `inisheer-gardens-dry-stone-walls-2002.jpg` — field-scale reference for
  irregular dry-stone wall networks.

## Regional Boundary Prior

Do not treat every Irish field boundary as a stone wall. Boundary material
varies strongly by landscape, geology, reclamation history, and local farm
practice.

For County Roscommon, use hedgerows, banks, and ditches as the default rural
field-boundary prior unless the map/control or landscape context gives a
stronger wall cue. The Roscommon Landscape Character Assessment identifies
`Rectilinear Fields - Hedgerows` as the predominant historic landscape type in
the county, with field boundaries made up of hedgerows with possible banks and
ditches. It separately notes `Rectilinear Fields - Stone Walls` for several
areas where stone walls were and remain typical field delineation.

Local Roscommon stone-wall emphasis is plausible in particular landscapes. The
same LCA describes Mid Lough Ree Pastureland as very well drained and having a
strong stone-wall character, especially around Knockcroghery. By contrast, the
county hedgerow survey estimated 15,574 km of hedgerow plus 2,165 km of remnant
hedgerow in Roscommon, and describes Roscommon's hedges as a distinctive county
feature. For Grove/Kilteevan-style Roscommon renders, the safer default is a
mixed boundary system:

- hedgerows, scrubby hedgebanks, banks, ditches, and remnant hedges for most
  ordinary field and garden divisions,
- stone-earthen banks where stone appears in a hedge/bank boundary,
- low broken dry-stone remnants or short full walls near gates, yards, roads,
  buildings, and well-drained/rocky patches,
- continuous full dry-stone walls only where the source/control or known local
  landscape justifies them.

For western/upland/rock-rich counties and subregions, stronger dry-stone wall
density is more plausible. Teagasc frames dry-stone walls as characteristic of
Irish rural and upland landscapes and notes very large national extents of both
dry-stone walls and stone-earthen banks. The Mayo hedgerow booklet gives a
useful regional contrast: hedgerows are common in east and south-east Mayo and
less common in the west, while hedges are scarcer in uplands and blanket-bog
areas. That supports using more stone walls in rocky western/upland places and
more hedgerows/banks/ditches in lowland agricultural landscapes.

## Stone-Earthen Banks

Use stone-earthen banks as the hybrid boundary type when a render needs "some
rocks, but not an all-stone wall." Teagasc describes this as a bank of soil or
ditch faced with stone on one or both sides, often planted on top with native
hedgerow plants such as hawthorn, ash, elm, alder, or furze. Visually, this is
better for much of Roscommon than a continuous wall grid: grassy earth bank,
irregular hedge growth, exposed stones along the face or base, gaps, and short
collapsed dry-stone patches.

## Visual Characteristics

Authentic Irish dry-stone field walls should read as:

- dry fit, with no mortar lines or cement seams,
- made from stones "taken as found" rather than cut into matching blocks,
- locally varied: limestone slabs in Burren/Aran-like terrain, rough fieldstone
  or bouldery walls elsewhere,
- uneven in height, edge, and silhouette,
- mixed stone sizes and orientations, including wedge-shaped, angular, rounded,
  slabby, and bouldery pieces,
- visible gaps, shadow pockets, chinks, and irregular interlock,
- grass, moss, lichen, weeds, and weather staining in crevices and wall bases,
- sometimes capped by uneven coping stones or upright/tilted stones,
- sometimes partly collapsed, patched, or interrupted by gates/openings,
- low and stock-boundary-like unless the map/control clearly supports a higher
  walled enclosure.

They should not read as:

- uniform rectangular stone blocks,
- brick-like courses,
- castle/estate ashlar masonry,
- smooth quarry-cut blocks,
- identical round gray beads,
- regular cobblestone strips,
- clean retaining walls with straight machine-cut faces,
- continuous perfect garden fortifications.

## Prompt Fragment

Use this fragment in reusable render prompts:

```text
Regional Irish boundary authenticity:
Do not turn every field boundary into a stone wall. For rural County Roscommon,
default ordinary field and garden boundaries toward hedgerows, hedgebanks,
ditches, banks, remnant hedges, and stone-earthen banks unless the source map,
control, gate/yard context, or local landscape clearly supports a full stone
wall. Full dry-stone walls should be local and source-supported, not a universal
grid. Stone-earthen banks may show grassy earth banks faced with irregular
stones and topped with hawthorn/ash/elm/alder/furze-like hedge growth.

Irish dry-stone wall authenticity:
Where a stone wall is source-supported, render it as a historic Irish
dry-stone / fieldstone wall: mortarless, dry fit, irregular, locally gathered
stones placed as found, with mixed sizes and shapes, visible gaps and shadow
pockets, uneven coping, moss, lichen, weeds, grass at the base, and a broken
hand-built silhouette. Use Burren/Aran-like slabby limestone only when
appropriate; otherwise use rough local fieldstone or mixed glacial fieldstone.
Do not render uniform rectangular blocks, brick-like courses, castle ashlar,
smooth cut masonry, identical gray beads, tidy cobblestone chains, or perfect
rectilinear block walls. Garden and field boundaries may be hedges, ditches,
banks, broken dry-stone walls, or overgrown remnants; do not make every
boundary a continuous stone wall.
```

## Audit Rule

Reject or repair a candidate if prominent walls look like uniform blockwork.
For a plate to pass, at least the foreground and central walls must show mixed
stone size, imperfect stacking, visible gaps, and organic edge variation.
