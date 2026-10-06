# Dependency security remediation (#2185)

The remediation upgrades vulnerable production Endpoints dependencies first,
then the engine and development toolchains. It retains narrow, documented OSV
exceptions where no compatible fix exists. No provider credential is committed.

## Prevention

The existing OSV workflow already scans all of main on pushes and a weekly
schedule, and its latest main run failed on the backlog. PR scans compare the
base and head, so they do not enforce removal of existing vulnerabilities.
The full scan now runs daily and supports manual dispatch; it continues to fail
on every unaccepted advisory. Exceptions live beside their affected lockfiles,
so both CI and `osv-scanner scan source -r .` apply them without extra flags.
The workflow's scoped agent guide records that both scan modes must remain.

## Dependency changes

Endpoints production resolves grpc-js 1.14.5, fast-uri 3.1.8/4.2.1,
ip-address 10.7.3, and brace-expansion 2.1.7. Its build-tool brace-expansion
branches also move to fixed patch releases. The root, legacy UI, promptfoo,
benchmark site, Opencode, Python tooling, and Rust lockfiles receive the
remaining available fixes. Overrides preserve direct package versions where
upstream still resolves vulnerable transitive versions.

Open Dependabot PRs #2176–#2179 and #2186–#2188 were checked for overlap.
This remediation includes the relevant Python, UI, and promptfoo security fixes;
the unrelated Tauri and UI major upgrades remain outside its scope.

During the subsequent merge of main, a fresh scan identified
[GHSA-wq5f-xc86-pv6w](https://github.com/advisories/GHSA-wq5f-xc86-pv6w)
in Sharp 0.35.4 in both promptfoo toolchains. Their overrides now pin 0.35.5;
the Endpoints lockfile already resolved 0.35.5, and its security floor is raised
to prevent regression. The merged Python requirements retain main's newer Ruff
and mypy plus the fixed Pygments pin.

## Accepted advisories

The lockfile-scoped OSV configuration records five exceptions:

- braces has no fixed release; build-tool glob input is repository-controlled.
- glib 0.18 belongs to the Linux desktop GTK3 stack; its fix requires a coordinated
  stack upgrade and it is absent from the iPhone dependency tree.
- proc-macro-error is unmaintained with no fix; its desktop macro expansion runs
  at compile time on repository-controlled code.
- rsa has no fix for private-key timing attacks; the server only verifies JWTs
  using public keys and does not sign or decrypt with private RSA keys.
- node-forge has no fixed release for the reported advisory and is confined to
  the development-only promptfoo jks-js dependency.

Each entry must be reconsidered when upstream offers a compatible fix or when
its documented scope changes.

## Deployment prerequisite found during verification

The current server assumed every live deployment had an OpenAI key and created
both adapters unconditionally. Production enables Google Vertex AI alone, so
that assumption rejected the first upgraded revision before it could serve.
Startup now permits a Google-only deployment without an OpenAI key, while
requiring the key when its OpenAI model allowlist is populated. A configuration
regression covers both cases; live rollout checks the actual startup path.

## Verification record

The exact full-source OSV command reports no issues, with seven advisory
occurrences filtered by the five documented exceptions. `just verify` passed,
including the Rust lint/tests, scripted walkthrough, and legacy UI checks.
Endpoints format, lint, typecheck, test, and build passed: 126 tests passed;
eight opt-in database/provider tests were skipped. `just notices`, documentation
format checks, and workflow actionlint also passed.

Coverage also exposed a pre-existing clock-boundary test whose arrival anchor
included seconds elapsed while loading the fixture. Adding 59 seconds could
therefore enter the next minute. The test now normalizes its anchor to a full
minute and retains the 0/1/59/60-second assertions; no gameplay behavior changes.
The focused regression passed. The local tarpaulin 0.35.4 parser failed on the
installed LLVM profile format; upstream tarpaulin 0.37.5 also failed mapping
coverage sections. CI uses cargo-llvm-cov with the same 60.8% floor. The actual
CI coverage command supplies the final coverage evidence; neither failed
tarpaulin attempt is counted as a pass.

Final deployment and simulator live-suite receipts are recorded in the linked
pull request.
Simulator live gameplay is distinct from physical-iPhone acceptance. This change
adds no iPhone feature or binary dependency and does not require a TestFlight build.
