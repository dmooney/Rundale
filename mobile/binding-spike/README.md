# Swift/Rust binding spike

This is a deliberately throwaway Phase 2 binding experiment. It contains no
Parish gameplay code and is not a second engine. The purpose is to exercise the
foreign-function boundary demanded by [software-technical-vision §9.2](../../docs/product-specs/software-technical-vision.md#92-binding-strategy)
before the real mobile runtime is wrapped.

The spike recommends a small owned C ABI as the starting point for Swift 6 and
iOS 17. The ABI is the contract; the Swift adapter can expose an actor-isolated
API and can poll/read events after each serialized operation. The callback in
this harness exists to prove the required background-to-`MainActor` behavior,
not to require callbacks in the production presentation layer.

## Why the C ABI won this spike

UniFFI is still a reasonable candidate for a later generated binding. Its
official Swift documentation provides useful value/error mappings and generates
the C header/module map plus the Swift wrapper. The current documentation also
states that Swift 6 support is partial and that generated async code is known
not to conform to `Sendable` (tracked as [uniffi-rs#2448](https://github.com/mozilla/uniffi-rs/issues/2448)).
That is a direct constraint for this project: the selected client is Swift 6,
the runtime has cancellation and late-result races, and the adapter must make
actor isolation explicit. The spike therefore keeps the semantic contract
small and hand-owned until generated async bindings demonstrate the same
behavior under the pinned toolchain.

The C ABI also avoids adding a generator, a proc-macro dependency, or a second
generated source lifecycle to the initial iOS integration. It does not expose
Rust structs, internal pointers, or arbitrary engine calls. A future UniFFI
trial can target the same operations without changing the Swift presentation
contract.

## Owned ABI shape

The complete declaration is in
[`include/rundale_binding_spike.h`](include/rundale_binding_spike.h).

| Operation                                  | Contract exercised                                                                                    |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `rd_session_create` / `rd_session_dispose` | Opaque `UInt64` handle, explicit lifetime, drained background workers, invalid-after-dispose behavior |
| `rd_session_submit_utf8`                   | Borrowed UTF-8 bytes copied into Rust and an explicit request ID returned                             |
| `rd_session_submit_async`                  | Background callback delivery with a borrowed payload valid only during the callback                   |
| `rd_session_cancel`                        | Serialized cancellation decision and distinct repeated-cancel status                                  |
| `rd_session_complete`                      | A late terminal result after cancellation is rejected with no committed event                         |
| `rd_session_take_json_batch`               | Owned JSON array bounded by the caller's maximum byte count; event paging preserves order             |
| `rd_owned_bytes_free`                      | Exactly-once release of every Rust-owned returned buffer                                              |

Recoverable failures return `rd_status_t`; no Rust panic is allowed to unwind
through the foreign boundary. Input, output, and batch limits are explicit in
the Rust implementation. Swift never retains an internal Rust pointer. The
callback receives provisional and terminal/late-ignored events from a Rust
worker thread; the harness records that fact and schedules each event on a
`MainActor` sink.

The intended production wrapper can keep the same ABI while exposing
`MobileSession` operations through one Swift actor: Rust owns the opaque
session and JSON/event contract, while the actor serializes calls and applies
events to the presentation model. URLSession transport and lifecycle
cancellation remain platform adapter concerns.

## Run the host proof

From this directory:

```sh
./run.sh
```

The script builds the static library with the repository-pinned Rust 1.98.0
toolchain, compiles the Swift 6 command-line harness with warnings as errors,
and runs it. It explicitly sets a local Cargo target directory and disables the
host's shared `sccache` wrapper because those paths are not writable in the
managed worktree. It does not install Rust targets and does not invoke Xcode.

The observed arm64 macOS result on 2026-09-07 was:

```text
PASS owned UTF-8, errors, and disposed handles
PASS bounded JSON event batches
PASS repeated create/dispose
PASS cancellation and late result rejection
PASS background callback and MainActor delivery
RESULT status=pass groups=5 assertions=730
```

This is a macOS host proof only. It does not prove that the Rust library links
for `aarch64-apple-ios` or `aarch64-apple-ios-sim`, that an XCFramework is
packaged correctly, or that a physical iPhone starts the runtime. Root-level
Phase 2 integration must build those actual targets and run the simulator and
device gates.

The host environment exposes Homebrew Rust 1.95.0 on `PATH`, while the
repository's `rust-toolchain.toml` selects 1.98.0 and the iOS targets are
managed by rustup. The script uses `rustup run 1.98.0` explicitly so the host
proof uses the pinned compiler; the target/device build should make the same
choice.

## Deliberate limits

This fixture uses a bounded in-memory event queue and deterministic worker
delays only to make callback races reproducible. It does not model persistence,
Endpoint streaming, authentication, or the real Parish engine. It is evidence
for the shape and ownership of the FFI boundary, not Phase 2 acceptance.

### Sources

- [UniFFI Swift bindings](https://mozilla.github.io/uniffi-rs/next/swift/overview.html)
- [UniFFI functions and async support](https://mozilla.github.io/uniffi-rs/latest/types/functions.html)
- [UniFFI binding generation](https://mozilla.github.io/uniffi-rs/latest/bindings.html)
