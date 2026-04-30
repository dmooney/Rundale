# parish-input

Player input parsing and command interpretation.

## Purpose

`parish-input` converts raw text input into structured commands and
intent-resolution prompts for downstream game systems.

## Responsibilities

- Parse slash commands (save/load/status/provider/map/theme/etc.).
- Route natural-language input toward inference-backed intent parsing.
- Return typed command/intent values for orchestration layers.

## Notes

This crate should stay focused on parsing and normalization, not world mutation
or session lifecycle management.
