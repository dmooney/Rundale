# Disposable CI gate validation

This harmless documentation-only file exists on a never-merge validation
branch for [issue 1714](https://github.com/dmooney/Rundale/issues/1714).

Its commit removes every UI diff from the branch. The protected pull-request
workflow should therefore skip `UI Playwright e2e` while allowing the sole
required `CI gate` to pass after the ordinary fast checks succeed.
