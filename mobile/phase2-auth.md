# Phase 2 mobile Endpoint authentication

This note records the mobile credential boundary for the Phase 2 vertical
slice. It describes the implementation and separates the exercised simulator
path from the still-open physical-iPhone gate.

## Boundary

`FirebaseEndpointCredentialProvider` in
[`RundaleEndpointCredentials.swift`](Rundale/RundaleEndpointCredentials.swift)
is the only mobile-owned source of credentials for a Parish Endpoint request.
The `@MainActor` `EndpointCredentialProviding` protocol keeps the Firebase
SDK objects on the main actor and lets transport tests inject
`FakeEndpointCredentialProvider` without configuring Firebase.

`EndpointCredentials` contains two in-memory values:

- `Authorization: Bearer <Firebase Auth ID token>` is the per-user Firebase
  ID token issued for the configured Firebase project. Its Firebase audience
  is the `cottage-d6dc9` project, and it authenticates the anonymous Firebase
  user to Parish Endpoints; it is not an arbitrary model-provider ID token
  and it is not a provider credential.
- `X-Firebase-AppCheck: <App Check token>` proves that the request came from
  an accepted Rundale app instance. The provider never saves either value,
  prints either value, or includes either value in an error description.

Firebase errors are propagated to the caller. Configuration and empty-token
failures use safe, token-free errors. The transport owns request retry and may
ask for a forced refresh after a server-authentication failure; this boundary
does not silently turn an auth failure into an unauthenticated request.

Parish Endpoints validates both mobile credentials before resolving a configured
app-to-organization/Endpoint binding or invoking a provider. Mobile identity is
separate from creator Firebase authorization and existing consumer API-key
invocation. See [the integration handoff](endpoint/phase2-handoff.md) for the
streaming contract and deployed evidence.

## Lazy configuration

The app starts with local fixture/gameplay services and does not configure
Firebase. `FirebaseApp.configure(options:)`, anonymous Auth sign-in, and App
Check token acquisition happen only when a remote credential request calls
`credentials(...)`. Local commands therefore remain usable without network
access or Firebase availability.

The app uses Firebase Apple SDK 12.18.0 through Swift Package Manager with
`FirebaseCore`, `FirebaseAuth`, and `FirebaseAppCheck`. A locally supplied
`GoogleService-Info.plist` is loaded from the app bundle at the first remote
request. Download the registered Rundale iOS app configuration from Firebase
and place it at `mobile/Rundale/Resources/GoogleService-Info.plist` before
running XcodeGen. This path is ignored by Git; CI must supply it privately.
A checkout without the file supports local gameplay but cannot authenticate
remote requests. The expected configuration is:

- Firebase project: `cottage-d6dc9`
- Google app ID: `1:24861210203:ios:2df6bf4ed8c4828253b17e`
- Bundle ID: `com.rundale.mobile`

The client validates these values before configuring Firebase. The backend
must independently validate the Firebase Auth token's issuer, audience,
signature, subject, and expiry, and must allowlist the Rundale Firebase app ID
for App Check. Client-side validation is an early configuration check, not a
replacement for backend verification.

Keep the Firebase client configuration out of source control, including the
API key. API-key restrictions, Firebase Auth, and App Check remain necessary
regardless of how the file is supplied. No model-provider key, shared Endpoint
secret, or long-lived invocation credential belongs in the iOS target.

## App Check providers

The provider factory is installed before `FirebaseApp.configure(options:)`:

- Private simulator debug builds use `AppCheckDebugProvider` only under
  `#if DEBUG && targetEnvironment(simulator)`. Firebase reads the optional
  `AppCheckDebugToken` scheme environment variable (or the deprecated
  `FIRAAppCheckDebugToken` spelling). A token must come from private local/CI
  secret storage, must never be committed or logged by Rundale, and must never
  ship in a production build.
- Physical iOS 14 and later uses `AppAttestProvider`.
- Older supported iOS versions fall back to `DeviceCheckProvider`. The current
  iOS 17 deployment target therefore uses App Attest on supported hardware.

The App Attest capability and production entitlement/signing configuration
remain project and Apple Developer Portal work. A simulator debug token can
prove development wiring; it cannot prove production attestation.

## Acceptance status

The following evidence remains separate from source-level implementation:

1. The generated project built with Firebase 12.18.0 for simulator and an
   unsigned device target in the Phase 2 gate.
2. The live simulator exercised the debug provider with a privately registered
   token, real anonymous Auth, a completed stream, and an explicit Stop. No
   token is checked into the repository or recorded in evidence.
3. Still required: exercise App Attest on a signed physical iPhone with the production
   entitlement and verify that Parish Endpoints accepts both headers.
4. Deterministic server tests cover wrong App Check app ID, missing App Check,
   malformed/missing bearer credentials, strict bindings, tenant/version
   hiding, quotas, switches, and rate limits. Live missing-auth rejection is
   recorded; additional live expired/revoked identities are not available and
   remain deterministic evidence.
5. The local/offline launch path remains covered without invoking Firebase.

These checks must be reported as live/device evidence only when they are
actually run; deterministic fake-provider tests do not satisfy them.
