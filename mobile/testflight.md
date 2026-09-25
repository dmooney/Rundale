# Internal TestFlight archive

From the repository root:

```sh
just mobile-build
just testflight-update
```

`mobile-build` builds the embedded Rust framework, generates the Xcode project,
and builds an unsigned Release iPhone app. `testflight-update` first runs
`./verify --phase all`, increments the local build number, then builds, signs,
inspects, and uploads an internal-only archive. Xcode manages the uploaded build
number to accommodate App Store Connect's existing builds. Check the actual
uploaded number before reporting it to the user. Applicable feature-specific
checks and save compatibility checks still apply.

Both commands write logs and build products under ignored mobile output paths.
Use `just mobile-build --dry-run` or `just testflight-update --dry-run` to preview
the commands without building, editing the version, or uploading. The commands
require the configured Xcode account, local Firebase file, XcodeGen, and Rust
toolchain described below. An unsigned local build does not require signing.

An upload is not yet an installable update: Apple must finish processing it, and
export compliance prompts may require completion in App Store Connect. The
command reports that distinction; the releasing agent must finish the portal
steps below and confirm **Testing** in **Internal Beta** before declaring delivery.

## Manual release and recovery

The Release build reads its Endpoint deployment identity from the app bundle.
The nonsecret deployment values are supplied as Xcode build settings, with no
production origin default compiled into the source. `LaunchConfiguration` still gives test
process environment values precedence over the bundle.

Supply the ignored Firebase file described in [phase2-auth.md](phase2-auth.md)
before generating the project. Sign in with the membership account in Xcode's
Apple Accounts settings. The existing membership team is `MBPRPZ283R`, the bundle
identifier is `com.rundale.mobile`, and the App Store Connect app is `6811694290`.
Reuse that record and its **Internal Beta** group with automatic distribution.

Check App Store Connect for the highest uploaded build number, then increase
`CURRENT_PROJECT_VERSION` in `mobile/project.yml` before generating the project.
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
  RUNDALE_ENDPOINT_SLUG=rundale-dialogue \
  RUNDALE_ENDPOINT_VERSION=1 \
  RUNDALE_INTENT_ENDPOINT_SLUG=rundale-intent \
  RUNDALE_INTENT_ENDPOINT_VERSION=1
```

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
iPhone App Attest. Keep physical-device results in [acceptance.md](acceptance.md).
