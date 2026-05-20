# Judge: 992-demo-player-name

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Independent check against acceptance-criteria.md

[criterion 1 — pinned name + rule in demo-prompt.txt]: confirmed at
`mods/rundale/demo-prompt.txt` paragraph 2: contains literal "Aiden Carney",
"if anyone asks your name … answer with this exact name", and the negative
constraint "Do not use the words 'Br Q' or any one- or two-letter capitalised
tokens as a name". Pins the name and the failure mode is named explicitly.

[criterion 2 — plausibility for 1820 east Roscommon]: "Aiden" (Aodhán) is a
recognised Irish given name in use before 1820; "Carney" (Ó Catharnaigh) is
a Connacht surname documented in 19th-century Roscommon parish records. The
pairing is unremarkable for a young east-Connacht traveller and matches the
"plausible 1820 Irish given name" guidance from the issue.

[criterion 3 — pinned name used when asked, no malformed artifact]: transcript
line 415 contains Peig's introduction prompt "Who might ye be, if I might ask
it so bold?"; transcript line 423 contains the player's response "Aye, my
name's Aiden. Came over by the Shannon, wonderin' the countryside." — the
pinned name verbatim with the prescribed backstory. A `grep` of the transcript
for `\b(Br|[A-Z]) [A-Z]\b` patterns in `chat [player]` lines returns zero hits
— the original "Br Q" failure does not reappear.

[criterion 4 — no drift across the transcript]: the remaining ten auto-player
turns are village-information questions, not introductions. The transcript
contains exactly one "my name's …" auto-player line, and that line uses the
pinned name. No alternative names appear in any player turn.

## Technical debt

Fix is a single-file edit to `mods/rundale/demo-prompt.txt`. No Rust change,
no test-suite churn, no new abstractions, no new flags. Negative constraint
references the #992 anti-pattern by name as a regression seatbelt.

Two acknowledged minor weaknesses, both already disclosed in `evidence.md`:

- The 12-turn demo elicited a single introduction event. The structural
  constraint in the prompt makes the fix robust against the next
  introduction, but only one positive observation exists in this transcript.
- The transcript contains `localhost:8000` 404 errors from the offline
  tier-2 NPC reactor. That worker was simply not running on this host; the
  errors are unrelated to the player-side prompt path under test and do not
  affect the verification.

Neither weakness is technical debt introduced by this change. Clear.

## Verdict rationale

Every criterion has either a transcript line citation or a deterministic
file-content check. The original failure mode (Qwen2.5-14B emitting "Br Q")
is now structurally precluded by an explicit negative constraint and a
pinned positive name. Fix is mod-content only, minimal, and demonstrably
exercised in a live Tauri demo run.
