# PR Quality Harness
Reusable GitHub PR quality gate that wraps normal checks with AI test-audit + mutation smoke verification.

## Setup
1. Copy `quality-harness/` to your repo.
2. `cp quality-harness/quality-harness.example.yml quality-harness.yml` and customize commands.
3. Add `OPENAI_API_KEY` secret.
4. Use reusable workflow `quality-harness/.github/workflows/pr-quality.yml`.

## What it does
- Runs install/lint/typecheck/tests from config
- Collects structured PR context into `.quality-harness/context.json`
- Calls AI hostile auditor and validates strict JSON
- Runs mutation smoke checks and ensures mutants are killed
- Publishes markdown/json artifacts and updates one PR comment
- Fails PR when audit/mutation gates fail

## Branch protection
Require the workflow status check (`PR Quality Harness`) before merge.

## Limitations
- Mutation operators are conservative regex transforms.
- Related-test mapping depends on your `commands.related_tests` implementation.
- AI assessment quality depends on prompt + model.
