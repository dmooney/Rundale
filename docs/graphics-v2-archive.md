# Graphics V2 Archive

Graphics V2 was an exploratory graphical-client and art-research effort. Its
map annotator, rendering scripts, concept rounds, generated experiments, and
review packets have been removed from the active repository. The alternative
notebook prototype was retired; the maintained chat-first interface remains. The full
research corpus, commit history, and source paths remain on the preserved
[`feat/graphics-v2` branch](https://github.com/dmooney/Rundale/tree/feat/graphics-v2). Its preservation PR is [#2159](https://github.com/dmooney/Rundale/pull/2159).
This branch is archival recovery material; the current product continues to use
the maintained chat-first interface and mobile client.

The current NPC person-art pipeline intentionally retains only the source inputs
needed by maintained generation and metadata review:

- `limerick/apps/ui/art/notebook-person-art/references/illustrated-rundale-notebook.png`
  is the canonical visual direction image. Its SHA-256 is
  `a34d088a9edc5d6cbe5d9d2e611fa40ce9d606140fae3258ef000a5638f84c81`.
- `limerick/apps/ui/art/notebook-person-art/references/art-metadata-guidelines.md`
  contains the NPC art metadata guidance. Its SHA-256 is
  `e22dc397d0989cacd78b4bd744f29e4af03e483732453a191fde0607264d5d67`.
- Approved art derivatives, manifests, source data, and runtime UI assets remain
  in their maintained locations. No approved files were rewritten.

Approved generation and review records keep their original historical paths,
including the former docs/graphics-v2 source and candidate review-packet
locations. Those paths describe where the immutable evidence was created; the
original content can be recovered from `feat/graphics-v2`. The active generation
configuration and prompt files use the retained reference path above.

The removed research and candidates can be browsed at the preserved branch:
[graphics-v2 corpus](https://github.com/dmooney/Rundale/tree/feat/graphics-v2/docs/graphics-v2),
[person-art experiments](https://github.com/dmooney/Rundale/tree/feat/graphics-v2/limerick/apps/ui/art/notebook-person-art/experiments),
and [person-art review packets](https://github.com/dmooney/Rundale/tree/feat/graphics-v2/limerick/apps/ui/art/notebook-person-art/review-packets).
