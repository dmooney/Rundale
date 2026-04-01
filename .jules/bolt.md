# Bolt's Journal

## 2026-03-28 - Pre-existing test breakage in inference module
**Learning:** The `max_tokens` parameter was added to `build_request()` and `InferenceQueue::send()` signatures but several tests weren't updated. This means test-only compilation failures can lurk undetected if `cargo test` isn't run regularly after API changes.
**Action:** When modifying function signatures, always grep for all call sites including test modules.
