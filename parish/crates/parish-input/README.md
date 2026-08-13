# parish-input

Player input parsing and command interpretation.

## Purpose

`parish-input` converts raw text input into structured commands and
intent-resolution prompts for downstream game systems.

## Responsibilities

- Parse slash commands (save/load/status/provider/map/theme/etc.).
- Route natural-language input toward inference-backed intent parsing.
- Extract `@mention` targets (e.g. `@Padraig Darcy`) from player input for addressed dialogue.
- Return typed command/intent values for orchestration layers.

## Slash command policy

- `/new` and `/new-game` both start a fresh game. The latter spelling also
  matches the HTTP API route, but input classification resolves it locally
  before gameplay routing.

## Notes

This crate should stay focused on parsing and normalization, not world mutation
or session lifecycle management.
