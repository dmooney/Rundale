# Internal TestFlight archive

From the repository root:

```sh
just mobile-build
just testflight-update
```

`mobile-build` builds the embedded Rust framework, generates the Xcode project,
and builds an unsigned Release iPhone app. `testflight-update` first runs
`just mobile-verify --phase all`, which reuses suites already passed with identical
inputs (it leaves most Rust crate tests and the Swift style checks to CI, so
upload a merged commit, or first run `just mobile-verify --fast`,
`cargo test -p limerick-core --no-default-features --features mobile`,
`cargo test -p limerick-mobile-ffi` and `cargo test -p limerick-engine --test world_sheet`
from `limerick/`), then builds, signs, inspects, and uploads an internal-only archive. The
archive's build number is passed to `xcodebuild` as `CURRENT_PROJECT_VERSION`, so
an upload edits no file and leaves nothing to commit; the value in
`mobile/project.yml` is only for local builds. With an App Store Connect API key
(`ASC_KEY_PATH`, `ASC_KEY_ID`, `ASC_ISSUER_ID`, as the GitHub workflow has), the
number is one above the highest build App Store Connect already holds, so the
logged number is the one TestFlight shows, whichever branch or machine uploads.
Xcode is told not to renumber it, so if another upload takes that number first the
upload fails rather than landing under a different one. `--dry-run` skips the
lookup.
Without a key it falls back to HEAD's commit count (`git rev-list --count HEAD`),
and Xcode may renumber the upload to follow App Store Connect's existing builds;
check the actual uploaded number before reporting it to the user. Applicable feature-specific
checks and save compatibility checks still apply.

Before the upload, `testflight-update` also exports the archive locally as a
distribution-signed `.ipa` (`mobile/.build/release/distribution-export/`) and
checks that copy: one app, a distribution signature (Apple Distribution, or
the legacy iPhone Distribution) that `codesign` verifies, with the chain
written to `release.log`, the expected build number and bundle metadata, and no provider or
Endpoint credential anywhere in the bundle. The upload export re-signs the
same archive the same way but keeps no local copy, so this is the inspectable
form of what TestFlight receives. A failed check stops the upload.

Both commands write logs and build products under ignored mobile output paths.
Use `just mobile-build --dry-run` or `just testflight-update --dry-run` to preview
the commands without building, editing the version, or uploading. The commands
require the configured Xcode account, local Firebase file, XcodeGen, and Rust
toolchain described below. An unsigned local build does not require signing.

An upload is not yet an installable update: Apple must finish processing it, and
export compliance prompts may require completion in App Store Connect. The
command reports that distinction; the releasing agent must finish the portal
steps below and confirm **Testing** in **Internal Beta** before declaring delivery.

## Uploading from GitHub

`.github/workflows/testflight.yml` uploads from a hosted `macos-26` runner on every
merge to `main` that changes `mobile/`, `mods/`, the engine crates or the Rust
toolchain, and on **Run workflow** from `main`. CI's required gate passed the PR first,
with a recorded `mobile-verify` pass or the hosted smoke gate (see the
[mobile scripts README](scripts/README.md#recorded-passes)); it tested the PR
head's input tree, which the squash merge matches unless `main` moved under it.
The `upload` job runs `release.py testflight --verify none` and goes straight to
the Rust build, archive, distribution export check and upload, about 15 minutes.
A push that leaves every app input unchanged (its input tree equals `main`'s
before the push, `verify_record.py tree`) uploads nothing.

The workflow's nightly `gate` job (07:41 UTC) uploads nothing and runs outside
the `testflight` environment and the upload's concurrency group. It checks what
the smoke gate left out, after the build shipped: when `main`'s whole input tree
has no recorded full pass (`check --latest`), it runs `verify.py --phase all`
(over an hour on this runner) and records the pass, so the next night skips. A
failed run opens or updates a "Nightly mobile-verify failed" issue with the
summary, and a passing run closes it. Run workflow uploads only from `main`. The Rust target directory is
cached between runs. `just testflight-update` keeps the full gate by default.
`release.py` signs with an App Store Connect API key when `ASC_KEY_PATH`,
`ASC_KEY_ID` and `ASC_ISSUER_ID` are all set, and with the signed-in Xcode account
otherwise.

The workflow reads these secrets from its `testflight` environment:

| Secret                                   | Value                                                                    |
| ---------------------------------------- | ------------------------------------------------------------------------ |
| `ASC_API_KEY_P8`                         | The contents of the App Store Connect API key's `.p8` file               |
| `ASC_KEY_ID`                             | That key's ID                                                            |
| `ASC_ISSUER_ID`                          | The team's issuer ID (App Store Connect, Users and Access, Integrations) |
| `FIREBASE_GOOGLE_SERVICE_INFO_PLIST_B64` | `base64 -i mobile/Rundale/Resources/GoogleService-Info.plist`            |

Create the key with the **Admin** role: automatic signing on a fresh runner
creates and fetches the distribution certificate itself. As with a local upload,
the run ends at "upload requested"; Apple's processing and the Internal Beta
**Testing** status are confirmed in App Store Connect.

## Reporting bugs from a beta build

Beta builds (`RUNDALE_BETA_FEEDBACK=YES`, which `testflight-update` sets and
checks) report bugs in one step (issue #2022,
[plan](../docs/plans/mobile-bug-report.md)): type `/bug` and what went wrong (`/bug the miller ignored me`), or shake
the phone. The app sends a screenshot and the engine's report to
`limerick-bug-report` (`RUNDALE_BUG_REPORT_URL`, also set and checked by
`testflight-update`) and says "Bug report sent. Thank you." A report made
offline is sent at the next launch. Reports wait in a private inbox until the
[`bug-triage`](../.agents/skills/bug-triage/SKILL.md) skill files the ones that
need an issue. An App Store build must set `RUNDALE_BETA_FEEDBACK=NO`.

## Manual release and recovery

The Release build reads its Endpoint deployment identity from the app bundle.
The nonsecret deployment values are supplied as Xcode build settings, with no
production origin default compiled into the source. `LaunchConfiguration` still gives test
process environment values precedence over the bundle.

Supply the ignored Firebase file, `mobile/Rundale/Resources/GoogleService-Info.plist`
for the `limerick-prod` project, before generating the project. The release
script refuses a file for any other Firebase project. Sign in with the membership account in Xcode's
Apple Accounts settings. The existing membership team is `MBPRPZ283R`, the bundle
identifier is `com.rundale.mobile`, and the App Store Connect app is `6811694290`.
Reuse that record and its **Internal Beta** group with automatic distribution.

Pass a build number higher than App Store Connect's highest upload as
`CURRENT_PROJECT_VERSION=<n>` on the archive command (`testflight-update` takes
App Store Connect's highest plus one when it has an API key); do not edit `mobile/project.yml` for it.
Run the applicable checks and verify save compatibility for persistence changes.
Rebuild the Rust framework when Rust or its build inputs have changed; reuse a
verified current framework otherwise. Archive with the recorded deployment identity:

```sh
bash mobile/scripts/build-rust-mobile.sh
xcodegen generate --spec mobile/project.yml
xcodebuild -project mobile/Rundale.xcodeproj \
  -scheme Rundale -configuration Release -destination 'generic/platform=iOS' \
  -archivePath mobile/.build/Rundale.xcarchive -allowProvisioningUpdates archive \
  DEVELOPMENT_TEAM=MBPRPZ283R CODE_SIGN_STYLE=Automatic \
  RUNDALE_ENDPOINT_BASE_URL=https://limerick-endpoints-877612517009.us-east1.run.app \
  RUNDALE_ENDPOINT_ORGANIZATION=limerick-demo \
  CURRENT_PROJECT_VERSION="$(git rev-list --count HEAD)"
```

The engine names each Endpoint role and version from the world mod
(`mods/rundale/endpoints/`), so only the origin and organization are build settings.

Select the Apple Development team for local device checks. An App Store
distribution certificate and provisioning profile are required to export this
archive for TestFlight; those signing settings are local. Upload only after
inspecting the archive and selecting the appropriate App Store Connect export
options.

Inspect the archived app's executable, version/build, Endpoint configuration,
Firebase resource, and signing entitlements before uploading. Keep signing
credentials and the ignored Firebase configuration out of version control.

Create an ignored `mobile/.verification/TestFlightExportOptions.plist` containing:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>upload</string>
  <key>teamID</key><string>MBPRPZ283R</string>
  <key>signingStyle</key><string>automatic</string>
  <key>testFlightInternalTestingOnly</key><true/>
  <key>manageAppVersionAndBuildNumber</key><true/>
  <key>uploadSymbols</key><true/>
</dict>
</plist>
```

```sh
xcodebuild -exportArchive \
  -archivePath mobile/.build/Rundale.xcarchive \
  -exportOptionsPlist mobile/.verification/TestFlightExportOptions.plist \
  -exportPath mobile/.build/testflight-export -allowProvisioningUpdates
```

Wait for Apple processing, resolve any compliance prompts accurately, and verify
the new build is **Testing** in **Internal Beta**. The existing tester is
`apple@dmooney.org`; a new invitation is unnecessary. The initial build contained
standard encryption outside Apple OS (`ring` AES-GCM/ChaCha20-Poly1305) and was
declared for internal distribution without France.

### Encryption declaration

The app's `Info.plist` declares `ITSAppUsesNonExemptEncryption` as Boolean `false`.
This records exemption from Apple's documentation requirement; it does not claim
that the app contains no encryption. The release validator checks the packaged
app for this exact Boolean before upload, including rejecting a missing key or a
string such as `"NO"`.

The September 14, 2026 review covers the current internal beta without France:
Endpoint requests use Apple `URLSession` HTTPS; Firebase provides authentication
and App Attest; the Rust dependency tree includes standard TLS cryptography via
`reqwest`/`rustls`/`ring`. No proprietary encryption is implemented by the game.
Apple's [documentation requirements](https://developer.apple.com/help/app-store-connect/reference/app-information/export-compliance-documentation-for-encryption)
list no upload for Apple OS encryption, and a French declaration for standard
non-Apple algorithms only when distributing on the App Store in France.
Apple's [property-list guidance](https://developer.apple.com/documentation/security/complying-with-encryption-export-regulations)
permits `false` when the app and linked libraries use only encryption exempt from
documentation requirements.

Reassess this declaration before changing encryption, SDKs, or distribution,
especially adding France. If documentation becomes required, update both the
plist and release validation and supply Apple's reviewed compliance code.
This key does not settle any separate government reporting obligations.
Previously uploaded builds retain their original metadata and may still prompt.

Tell the user the version/build, what changed and what to try, actual checks,
and remaining device validation. They can use **TestFlight → Rundale → Update**
or enable automatic updates. Verify App Attest, dialogue, and local save/resume
on the iPhone separately from upload success.

An unsigned build proves packaging only; it cannot be uploaded or installed
through TestFlight. Simulator debug authentication does not prove physical
iPhone App Attest. Record physical-device results with the release, as the
[test plans README](../docs/test-plans/README.md) describes.
