# Input Enrichment Designs

> Status: historical feature designs. The implementation plan is complete; use
> these documents to understand interaction intent and constraints, not as a
> claim that every experiment is enabled in the current UI.

These focused designs support the player-input system. Start with the durable
[Player Input design](../player-input.md) for the current engine behavior and
the [completed implementation plan](../../plans/archive/input-enrichment-implementation.md)
for rollout context.

| Design                                                | Focus                               |
| ----------------------------------------------------- | ----------------------------------- |
| [01 Slash Autocomplete](01-slash-autocomplete.md)     | Discoverable system commands        |
| [02 Emote Actions](02-emote-actions.md)               | Expressive action affordances       |
| [03 Input History](03-input-history.md)               | Reusing earlier player input        |
| [05 Whisper Syntax](05-whisper-syntax.md)             | Private / quiet dialogue syntax     |
| [06 Emoji Reactions](06-emoji-reactions.md)           | Reaction affordances                |
| [07 Quick Travel Buttons](07-quick-travel-buttons.md) | Navigation shortcuts                |
| [09 Smart Replies](09-smart-replies.md)               | Context-sensitive reply suggestions |
| [15 Tab-Complete Nouns](15-tab-complete-nouns.md)     | World-aware noun completion         |

For the current visual input surface, read the [Illustrated Notebook Real Play
Screen](../illustrated-notebook-real.md) and its
[implementation plan](../../plans/illustrated-notebook-real.md). It intentionally
keeps autocomplete and dropdown work out of its first Pixi slice so the legacy
input-field treatment does not leak into the default viewport.
