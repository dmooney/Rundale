# Gaeilge Slice Sources

The `gaeilge` slice is source-backed, not invented from scratch.

## Tatoeba

- Files used locally: `eng-gle_links.tsv.bz2`, `eng_sentences.tsv.bz2`,
  `eng_sentences_detailed.tsv.bz2`, `gle_sentences.tsv.bz2`, and
  `gle_sentences_detailed.tsv.bz2` from Tatoeba weekly exports.
- License: CC BY 2.0 FR for textual sentences.
- Attribution: each Tatoeba-backed record stores English and Irish sentence
  IDs plus contributor names where present in the detailed export.
- Use in slice: translation and idiom-variant records.

## UD Irish-IDT

- File used locally: `ga_idt-ud-train.conllu` from
  `UniversalDependencies/UD_Irish-IDT`.
- License: CC BY-SA 3.0.
- Attribution: each UD-backed record stores the UD `sent_id`.
- Use in slice: comprehension, grammar-transform, and idiom-explanation records.

## Corpas Naisiunta na Gaeilge

Corpas Naisiunta na Gaeilge is the right larger source for future expansion and
calibration, but this starter slice does not import CNG text. It is CC BY-SA 4.0
and should be added deliberately with explicit attribution and share-alike review
if we decide to check excerpts into the repository.
