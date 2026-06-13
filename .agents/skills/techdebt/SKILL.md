---
name: techdebt
description: 'Technical-debt reduction in two modes: (1) a continuous TODO.md sweeper that consumes/discovers debt and dispatches focused fix agents until none remains, and (2) a crate-layout audit that produces a behaviour-preserving refactor PR (renames → manifests → splits → extractions → README) for the Rust workspace. Use for ongoing debt cleanup or when auditing crate structure.'
disable-model-invocation: false
argument-hint: '[path] | crate-audit [phase]'
---

Two modes. The default **debt-sweeper loop** (below) consumes a `TODO.md` list for any scope. For a
structural workspace cleanup, jump to the **crate-layout audit** mode at the end.

Use the default mode to run a structured debt-reduction loop for a target file/folder. If no argument is provided, default to the current working directory.

## Inputs

- `$ARGUMENTS` (optional): file or directory path to scope work.
  - If omitted, use `.`.
  - If a file is provided, use its parent directory as debt-list home and keep analysis focused on that file.

Set `TARGET` from the argument (or `.`), then resolve:

- `SCOPE`: exact file/folder(s) agents should inspect/fix.
- `ROOT`: directory where `TODO.md` is read/written.

## Loop contract

Run this cycle repeatedly until exit criteria are met.

1. **Load or initialize debt list**
   - Look for `TODO.md` under `ROOT`.
   - If missing, create one with sections:
     - `## Open`
     - `## In Progress`
     - `## Done`
   - Keep entries concise and actionable, with stable IDs (`TD-001`, `TD-002`, ...), owner, and status.

2. **Choose work source**
   - If `## Open` has items, pick the highest-impact small batch (1–3 items).
   - If no open items exist (or everything is done), spawn discovery agents to scan `SCOPE` for technical debt:
     - dead/unreachable code
     - duplication and abstraction opportunities
     - weak or missing tests
     - stale docs/comments/config
     - high-complexity hotspots and brittle conditionals
   - Add each validated finding to `## Open` before fixing.

3. **Dispatch fix agents**
   - Spawn parallel agents when tasks are independent; otherwise run serially.
   - Give each agent exactly one debt item ID and acceptance criteria.
   - Require each fix agent to:
     - implement minimal, behavior-safe change
     - add/update tests when behavior or guarantees change
     - run relevant checks
     - report file list + commands run + residual risks

4. **Reconcile and update `TODO.md`**
   - Move started items to `## In Progress`, then to `## Done` only after checks pass.
   - For partially fixed work, keep item open with a narrowed remaining scope.
   - Remove duplicates; merge equivalent debt items under the earliest ID.
   - Append a short progress log entry (date + IDs completed).

5. **Gate before next loop**
   - Ensure repository is in a clean, buildable state for touched areas.
   - If new debt was discovered during fixes, record it under `## Open`.
   - Return to Step 2.

## Exit criteria

Stop only when all are true:

- `## Open` is empty.
- Discovery pass finds no credible new debt in `SCOPE`.
- No `## In Progress` items remain.

Then leave a final note in `TODO.md` summarizing what was checked and why the loop ended.

## Operating rules

- Keep tasks small and independently landable.
- Prefer deleting dead code over refactoring it.
- Do not invent speculative debt; every item needs concrete evidence (file/line/symptom).
- Preserve AGENTS.md rules (tests, feature flags, mode parity, docs updates).
- If uncertain whether something is debt vs intentional, record a question item instead of changing behavior.

---

## Crate-layout audit mode

Invoke with `crate-audit` (optionally a phase name, e.g. `crate-audit renames`). The goal is a **shippable,
behaviour-preserving refactor PR** that leaves the workspace easier to navigate. Mechanics matter: the most
common failure mode is mixing a real bug fix into a refactor commit and burning the trust that lets reviewers
fast-track these PRs.

**Inputs:** no required arguments. If the user names specific phases ("just the renames"), respect that
scope. Optional LOC threshold for big-file detection (default `1500`).

**Output:** one PR with up to four logical phases as separate commits. Phases that find nothing are skipped
silently — don't pad the PR.

### Step 1 — Baseline

Confirm the tree is clean and tests pass before touching anything. If `cargo test --workspace` is red on
`main`, stop and tell the user — refactoring on a broken baseline buries the cause.

```sh
git status                          # must be clean
cd parish && cargo build --workspace
cd parish && cargo test --workspace --lib
```

The Cargo workspace lives in `parish/` — run cargo from there (or via `just`). If there's a Tauri crate,
exclude it from local verification (`--exclude parish-tauri`) — it needs system libs CI handles. Note this in
the PR description.

### Step 2 — Phase 1: naming hygiene

Enumerate `crates/*` and look for:

1. **Missing workspace prefix.** If the convention is `parish-*`, every dir under `crates/` should match.
   Flag stragglers (`geo-tool/`, `npc-cli/`).
2. **Binary name vs. crate name drift.** Inside each `Cargo.toml`, check `[package].name` and `[[bin]].name`
   against the directory. Rename the laggard.
3. **Stale references.** After every rename, run `git grep -F "<old-name>"` across the whole repo (Rust,
   TOML, justfile, docs, deploy/, .github/). Zero hits is the gate. Don't trust IDE rename — text mentions
   in docs and CI configs slip through.

Each rename = one commit, prefix `refactor:`. Body lists every callsite class touched (workspace toml,
binary name, justfile recipes, docs, deploy artifacts).

### Step 3 — Phase 2: manifest standardization

For every `crates/*/Cargo.toml`:

- `description = "..."` — required, one line, mentions "Parish" or the engine for searchability.
- `edition = "2021"` (or the workspace standard) — must match across crates.
- `[lib]` block — present if `src/lib.rs` exists, with `name = "<crate_name_with_underscores>"` and an
  explicit `path = "src/lib.rs"` if any are inconsistent (consistency > brevity here).
- License field if the workspace uses one.

Pull existing descriptions in one pass:

```sh
for d in crates/*/; do
  desc=$(grep '^description' "$d/Cargo.toml" 2>/dev/null | head -1)
  printf "%-22s %s\n" "$(basename $d)" "$desc"
done
```

One commit, `chore: standardize Cargo.toml descriptions and [lib] blocks`. Skip if already consistent.

### Step 4 — Phase 3: big-file splits

Find single-file libs over the threshold:

```sh
find crates -name 'lib.rs' -o -name 'main.rs' | xargs wc -l | sort -n | tail -10
```

For each file over threshold:

1. **Read it end-to-end first.** Don't split blind. Identify natural module boundaries (commands vs. parsing
   vs. types vs. LLM-call vs. local-fastpath, etc.).
2. **Plan the split.** Write the target module list before moving code. 4–8 modules is the sweet spot; one
   per major concern.
3. **Move, don't rewrite.** Each new module's contents should be **byte-identical** to the corresponding
   section of the old file. The new `lib.rs` becomes a glue file: `mod x; pub use x::Y;`. No logic changes,
   no rename, no reordering for "tidiness."
4. **Tests stay where they are.** A `#[cfg(test)] mod tests` block at the bottom of `lib.rs` can stay for
   the first pass — moving tests is its own follow-up. If tests reference now-private items, add a
   `pub(crate)` and note it.
5. **Verify byte-identity.** After the split, `git show <pre-split-sha>:<old-path>` and `cat` the
   concatenated new modules — diffs should be limited to module boundaries and `use` lines.

One commit per split: `refactor(<crate>): split single-file lib.rs into N modules`. Body lists the modules
and what each contains. If a split exposes a real bug (Gemini will find them), see Step 7.

### Step 5 — Phase 4: crate extraction candidates

Look for **self-contained leaf modules** that could become their own crate. All four must hold:

- **Leaf in the dep graph.** Imports only `parish-types` / external crates — no calls into siblings.
- **Distinct concern.** Used by multiple crates, or the parent's identity would be tighter without it.
- **Stable surface.** Public API is small and not in flux.
- **Worth the manifest tax.** A new crate adds Cargo.toml, README, CI surface — if < 200 LOC, the tax
  outweighs the win.

Good candidates from past audits: pure-data palette/color crates, ID/newtype crates, prompt-template
loaders. Bad candidates: anything with a `Database`, `Session`, or `World` reference. If extraction would
create a dependency cycle (e.g. types live in the parent crate), **defer** it and write a follow-up issue
describing the precursor work (move shared types to a leaf crate first). Do not paper over a cycle with `pub
use` re-exports. Extraction commit: `refactor: extract <new-crate> from <parent>`. Update the workspace
`Cargo.toml` members list and add the new crate to the README listing.

### Step 6 — Phase 5: README freshness

The repository-layout block in `README.md` must list **every** `crates/*` directory with a one-line
description matching the crate's `Cargo.toml description`. Order roughly bottom-up by dependency layer (types
→ config → leaves → core → binaries). This phase often catches the audit's only user-visible defect — a
README that documents 5 crates when there are 14.

### Step 7 — Pre-existing bugs surfaced during the refactor

Reviewers will flag bugs in the moved code. Most are pre-existing — the split just gave them a fresh diff.

1. **Verify pre-existing.** `git show <pre-split-sha>^:<old-path>` and check the same lines exist verbatim.
   Quote the pre-split sha and line range in your reply.
2. **Triage.** Real defect → file a follow-up issue with the proposed fix and file:line reference (title
   `<crate>: <one-line defect summary>`). Test-contracted behaviour → note it's explicitly tested and the
   fix needs a consumer audit + test updates; still a follow-up issue, but flag the ambiguity.
3. **Reply on the thread.** Brief, factual, one paragraph. Confirm pre-existing, link the follow-up issue,
   decline to fix in this PR.
4. **Never** mix the fix into the refactor PR. "This PR changes no behaviour" is what makes it cheap to
   review and safe to merge.

### Step 8 — Verification gates before push

In order:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings  # exclude parish-tauri locally
cargo test --workspace --lib --exclude parish-tauri
git grep -F "<every-renamed-thing>"                    # zero hits
```

Read the test count — it should be ≥ baseline. A drop usually means a `#[cfg(test)] mod tests` got orphaned
during a split.

### Step 9 — PR mechanics

- **Title:** `refactor: audit and tidy crate structure` (or scope-specific if narrower).
- **Body:** a section per phase listing what landed, then a "Deferred follow-ups" section listing extractions
  needing precursor work and bugs filed as separate issues. Link the issues by number.
- **Conflicts on rebase:** main moves; expect to rebase. The usual conflict is `Cargo.lock` (take main's,
  rebuild — cargo regenerates entries) plus dep-version bumps touching the same `Cargo.toml` lines. Resolve
  manually, keep both intents.
- **Merge:** wait for CI green. If new review comments arrive after the user approved the merge plan, follow
  the user's stated policy on whether to wait or merge through.

### Crate-audit failure modes to avoid

- **Mixing fixes with moves.** Ruins the byte-identity guarantee. Always separate.
- **Renames without a stale-grep gate.** Forgetting a Dockerfile or justfile recipe ships a broken main.
- **Splitting before reading.** Boundaries picked from filenames usually produce a worse layout.
- **Extracting into a cycle.** If the new crate would depend on its parent, the extraction is wrong. Move the
  cycle-causing types first.
- **README drift.** Easy to forget; reviewers rarely catch it; users notice immediately. Always include
  README updates in the same PR.
