# Inference configuration v2

Limerick has two non-secret configuration authorities. The project file is the
explicit `--config` path (or the startup-resolved project `limerick.toml`); the
user file is `<user-config-dir>/limerick.toml`. Both require
`schema_version = 2`. Secrets remain in the OS keychain or named environment
variables and are never serialized into either file.

Precedence is deterministic: compiled defaults, project, user, environment,
then CLI. Named `loadouts` and custom `providers` replace the entire entry at
the next layer; their nested fields are not deep-merged. Within a selected
loadout, a category route is resolved in this order: provider preset, loadout
default, global environment/CLI overrides, category route, and category
environment/CLI overrides. A subrole generation profile then applies after
its category route.

Model identity is always the triple `(provider_id, endpoint_id, model_id)`.
The endpoint selects the inference, discovery, authentication, reasoning, and
local-management adapters. A model name never selects a wire protocol by
heuristic. Reasoning is a separate typed intent (`auto`, `off`, `effort`, or
`budget`) and is accepted only when that exact model route declares a
translation and enough output-token headroom.

The inference catalog has four concrete network API flavors:
`openai-responses-v1`, `openai-chat-v1` (also the explicit compatibility
fallback), `anthropic-messages-2023-06-01`, and `google-interactions-v1`.
The in-process `simulator` is a fifth inference adapter, but it is not a
network API. Discovery is a separate adapter axis because `/models` shapes do
not determine the inference wire. Thinking is another separate axis:
`reasoning_dialect` translates the semantic `auto`/`off`/`effort`/`budget`
intent only for a specifically declared model route.

Legacy `[provider]` and `[cloud]` files are intentionally rejected rather than
silently reinterpreted. The loader error identifies the file, observed version,
and required version. There is no field migration command. Desktop startup
never changes an invalid file automatically: its native recovery dialog lets
the user retry after editing, reveal the exact file, or quit. User configuration
also offers a separately confirmed Archive and Reset action. That transaction
archives bytes as `.v1.bak.<UTC>` or `.invalid.bak.<UTC>` before atomically
installing a validated clean-v2 document. Project configuration is never reset
by the application. CLI validation and engine startup use `EX_CONFIG` (78) for
configuration failures.

The compiled `default` loadout is the keyless in-process simulator so a clean
installation can always reach onboarding. It is not a cloud recommendation:
recommendation requires a route-specific qualification receipt. Live model
discovery is account-isolated by an install-salted credential HMAC and the
canonical `(provider, endpoint, inference URL, discovery URL, adapter versions)`
identity. Only a fresh, complete listing can mark a missing model `not-listed`.
Runtime startup exact-loads only those active identities; the bounded catalog
index is for listing and maintenance. Discovery revalidation preserves root
ETag/Last-Modified validators, the original last-good observation time, and a
length-prefixed ordered-page payload hash. A changed normalized field retains
a bounded conflict record with both payload hashes instead of silently erasing
the prior observation.

Operational commands:

```sh
limerick config validate --project limerick.toml
limerick config validate --user /path/to/user/limerick.toml
limerick config show-effective --project limerick.toml --json
limerick catalog list
limerick catalog refresh --project limerick.toml
# Remote probes can incur cost and require explicit acknowledgement:
limerick catalog probe --category dialogue --billable-confirm --project limerick.toml
```

Catalog documents live under
`<user-data>/cache/model-catalog/v1`; immutable probe evidence lives under
`<user-data>/model-probes/attempts`. Every probe writes
`<id>/{request.json,raw-response.bin,receipt.json}`
transaction. The raw response is synced before terminal/schema validation;
the bounded observation index retains the newest 32 receipt references per
route without deleting the immutable attempts.
Authenticated disk caching requires an owner-only installation salt on Unix;
platforms where owner-only salt storage cannot be verified fail closed for
authenticated caching while anonymous catalog entries remain available.

Checked-in JSON Schemas:

- `docs/schemas/limerick-project-config-v2.schema.json`
- `docs/schemas/limerick-user-config-v2.schema.json`

Regenerate them with:

```sh
cd limerick
cargo run -p limerick-config --example generate_v2_schemas
```
