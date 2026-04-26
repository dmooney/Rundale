---
name: crate-audit
description: Audit the Rust workspace's crate layout for naming hygiene, manifest consistency, oversized files, extraction candidates, and README freshness. Produces a phased refactor PR (renames → manifests → splits → extractions) that is pure relocation — no behaviour changes. Use when the workspace has accumulated cruft, a new contributor is reading the crate map, or the user asks to "audit the crate structure".
---

The goal is a **shippable, behaviour-preserving refactor PR** that leaves the workspace easier to navigate. Mechanics matter: the most common failure mode is mixing a real bug fix into a refactor commit and burning the trust that lets reviewers fast-track these PRs.

## Inputs

- No required arguments. If the user names specific phases ("just the renames") respect that scope.
- Optional: a LOC threshold for big-file detection (default `1500`).

## Output

One PR with up to four logical phases as separate commits. Phases that find nothing to fix are skipped silently — don't pad the PR.

---

## Step 1 — Baseline

Confirm the tree is clean and tests pass before touching anything. If `cargo test --workspace` is red on `main`, stop and tell the user — refactoring on top of a broken baseline buries the cause.

```sh
git status              # must be clean
cargo build --workspace
cargo test --workspace --lib
```

If there's a Tauri crate, exclude it from local verification (`--exclude parish-tauri`) — it needs system libs CI handles. Note this in the PR description.

## Step 2 — Phase 1: naming hygiene

Enumerate `crates/*` and look for:

1. **Missing workspace prefix.** If the convention is `parish-*`, every directory under `crates/` should match. Flag stragglers (`geo-tool/`, `npc-cli/`).
2. **Binary name vs. crate name drift.** Inside each `Cargo.toml`, check `[package].name` and `[[bin]].name` against the directory. Rename the laggard.
3. **Stale references.** After every rename, run `git grep -F "<old-name>"` across the whole repo (Rust, TOML, justfile, docs, deploy/, .github/). Zero hits is the gate. Don't trust IDE rename — text mentions in docs and CI configs slip through.

Each rename = one commit. Conventional commit prefix `refactor:`. Body lists every callsite class touched (workspace toml, binary name, justfile recipes, docs, deploy artifacts).

## Step 3 — Phase 2: manifest standardization

For every `crates/*/Cargo.toml`:

- `description = "..."` — required, one line, mentions "Parish" or the engine for searchability.
- `edition = "2021"` (or whatever the workspace standard is) — must match across crates.
- `[lib]` block — present if `src/lib.rs` exists, with `name = "<crate_name_with_underscores>"` and an explicit `path = "src/lib.rs"` if any are inconsistent (consistency > brevity here).
- License field if the workspace uses one.

Pull the existing descriptions in one pass:

```sh
for d in crates/*/; do
  desc=$(grep '^description' "$d/Cargo.toml" 2>/dev/null | head -1)
  printf "%-22s %s\n" "$(basename $d)" "$desc"
done
```

One commit, `chore: standardize Cargo.toml descriptions and [lib] blocks`. Skip if everything's already consistent.

## Step 4 — Phase 3: big-file splits

Find single-file libs over the threshold:

```sh
find crates -name 'lib.rs' -o -name 'main.rs' | xargs wc -l | sort -n | tail -10
```

For each file over threshold:

1. **Read it end-to-end first.** Don't split blind. Identify natural module boundaries (commands vs. parsing vs. types vs. LLM-call vs. local-fastpath, etc.).
2. **Plan the split.** Write the target module list before moving any code. 4–8 modules is the sweet spot; one module per major concern.
3. **Move, don't rewrite.** Each new module's contents should be **byte-identical** to the corresponding section of the old file. The new `lib.rs` becomes a glue file: `mod x; pub use x::Y;`. No logic changes, no rename, no reordering for "tidiness."
4. **Tests stay where they are.** A `#[cfg(test)] mod tests` block at the bottom of `lib.rs` can stay there for the first pass — moving tests is its own follow-up. If tests reference now-private items, add a `pub(crate)` and note it.
5. **Verify byte-identity.** After the split, `git show <pre-split-sha>:<old-path>` and `cat` the concatenated new modules — diffs should be limited to module boundaries and `use` lines.

One commit per split: `refactor(<crate>): split single-file lib.rs into N modules`. Body: list the modules and what each contains.

If a split exposes a real bug (Gemini will find them), see Step 7.

## Step 5 — Phase 4: crate extraction candidates

Look for **self-contained leaf modules** that could become their own crate. Criteria — all four must hold:

- **Leaf in the dep graph.** The module imports only `parish-types` / external crates — no calls into siblings.
- **Distinct concern.** Used by multiple crates, or the parent crate's identity would be tighter without it.
- **Stable surface.** Public API is small and not in flux.
- **Worth the manifest tax.** A new crate adds Cargo.toml, README, CI surface — if the module is < 200 LOC, the tax outweighs the win.

Good candidates from past audits: pure-data palette/color crates, ID/newtype crates, prompt-template loaders. Bad candidates: anything with a `Database`, `Session`, or `World` reference.

If extraction would create a dependency cycle (e.g. types live in the parent crate), **defer** the extraction and write a follow-up issue describing the precursor work (move shared types to a leaf crate first). Do not paper over a cycle with `pub use` re-exports.

Extraction commit: `refactor: extract <new-crate> from <parent>`. Update the workspace `Cargo.toml` members list, add the new crate to the README listing.

## Step 6 — Phase 5: README freshness

The repository-layout block in `README.md` must list **every** `crates/*` directory with a one-line description that matches the crate's `Cargo.toml description` field. Order roughly bottom-up by dependency layer (types → config → leaves → core → binaries) so a reader can follow the layering.

This phase often catches the audit's only user-visible defect — a README that documents 5 crates when there are 14.

## Step 7 — Pre-existing bugs surfaced during the refactor

Reviewers (Gemini, Copilot) will flag bugs in the moved code. Most are pre-existing — the file split just gave them a fresh diff to comment on. Procedure:

1. **Verify pre-existing.** `git show <pre-split-sha>^:<old-path>` and check the same lines exist verbatim. Quote the pre-split sha and line range in your reply.
2. **Triage.**
   - **Real defect** (e.g. trailing punctuation polluting a parsed name, doc-comment contradicting code): file a follow-up issue with the proposed fix and the file:line reference. Title: `<crate>: <one-line defect summary>`.
   - **Test-contracted behaviour** (e.g. case-insensitive parsing): note that the behaviour is explicitly tested, and the fix needs an audit of consumers + test updates. Still a follow-up issue, but flag the ambiguity.
3. **Reply on the thread.** Brief, factual, one paragraph. Confirm pre-existing, link the follow-up issue, decline to fix in this PR. Don't argue the merits — the issue is the place for that.
4. **Never** mix the fix into the refactor PR. The discipline of "this PR changes no behaviour" is what makes it cheap to review and safe to merge.

## Step 8 — Verification gates before push

In order:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings  # exclude parish-tauri locally
cargo test --workspace --lib --exclude parish-tauri
git grep -F "<every-renamed-thing>"                    # zero hits
```

Read the test count. It should be ≥ baseline. A drop usually means a `#[cfg(test)] mod tests` got orphaned during a split.

## Step 9 — PR mechanics

- **Title:** `refactor: audit and tidy crate structure` (or scope-specific if narrower).
- **Body:** sections for each phase listing what landed, then a "Deferred follow-ups" section listing extractions that need precursor work and bugs filed as separate issues. Link the issues by number.
- **Conflicts on rebase:** main moves; expect to rebase. The usual conflict is `Cargo.lock` (take main's, rebuild — cargo regenerates entries for new crates) plus dep-version bumps that touched the same `Cargo.toml` lines you edited. Resolve manually, keep both intents.
- **Merge:** wait for CI green. If new review comments arrive after the user has approved the merge plan, follow the user's stated policy on whether to wait or merge through.

## Failure modes to avoid

- **Mixing fixes with moves.** Ruins the byte-identity guarantee that makes the PR cheap to review. Always separate.
- **Renames without a stale-grep gate.** Forgetting to update a Dockerfile or a justfile recipe ships a broken main.
- **Splitting before reading.** Picking module boundaries from filenames or first impressions usually produces a worse layout than the monolith.
- **Extracting into a cycle.** If the new crate would need to depend on its parent, the extraction is wrong. Move the cycle-causing types first.
- **README drift.** Easy to forget; reviewers rarely catch it; users notice immediately. Always include README updates in the same PR as the structural changes.
