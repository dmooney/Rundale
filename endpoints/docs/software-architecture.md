Parish Endpoints: Software Architecture
Status: Draft
Companion document: product-vision.md
Implementation approach: Goblin
Architecture phase: MVP with explicit evolution path to marketplace
________________


1. Purpose
This document defines the software architecture for a platform that turns configured AI behavior into stable, hosted, typed API endpoints.
A creator defines:
* an input contract,
* a model,
* private instructions,
* inference parameters,
* an output JSON Schema,
and publishes the definition as a callable hosted Endpoint.
A consumer invokes that function through a stable HTTP API without needing to know:
* which model provider is used,
* what prompt is used,
* how structured output is enforced,
* how provider credentials are managed,
* how retries are performed,
* how usage is metered.
The MVP must support the first production use case:
seed packet image -> Cottage SeedPacket JSON
The architecture must remain general enough to support later use cases such as Rundale’s NPC cognition and world-simulation functions without adding application-specific runtime code.
________________


1.1 Parish Product Boundary and Terminology
Parish is the working umbrella name and parish.dev is the working domain.
The Parish umbrella may include:
Parish
├── Parish Engine      # Rundale-derived game/simulation engine
└── Parish Endpoints   # hosted AI API product in this document
These are conceptually related systems with separate runtime boundaries and
deployables. Rundale vendors Endpoints as a self-contained nested pnpm
workspace without merging its dependencies into the Rust or player frontend
workspaces.
For this product, the first-class object is an Endpoint.
An Endpoint is a hosted HTTP API interface whose implementation is managed by Parish. The creator does not deploy arbitrary code. Instead, an Endpoint definition contains:
input schema
+ private/proprietary instructions
+ selected LLM/model
+ inference parameters
+ output schema
+ publication/version state
Invocation is:
consumer application
    |
    | HTTP request
    v
Parish Endpoint
    |
    | managed inference using private Endpoint Definition
    v
LLM provider
    |
    v
validated API response
This is deliberately different from AWS Lambda and conventional Function-as-a-Service:
Parish Endpoint
	Lambda/serverless function
	Primarily an HTTP API product
	Primarily executable compute
	Creator configures AI behavior
	Creator uploads/writes arbitrary code
	Private instructions + model + schemas
	Runtime + code package
	Hosted URL is the product surface
	HTTP is one possible trigger
	Managed LLM inference
	General-purpose event-driven execution
	No arbitrary creator code in MVP
	Arbitrary supported runtime code
	Terminology used throughout this architecture:
* Endpoint — the creator-owned hosted AI API object.
* Endpoint Definition — mutable draft configuration describing model, private instructions, schemas, and settings.
* Endpoint Version — immutable published snapshot.
* Endpoint URL — consumer-facing invocation URL.
* Parish API — creator-facing management/control API used to create, edit, publish, inspect, and administer Endpoints.
* Invocation API — the runtime surface that receives calls to published Endpoint URLs.
* Parish Engine — separate Rundale-derived game/simulation technology under the Parish umbrella.
________________


2. Architectural Goals
2.1 Primary Goals
The system must:
1. Allow creators to define Endpoints without writing backend serving code.
2. Publish an Endpoint as a stable authenticated HTTP endpoint.
3. Accept multimodal input, including an image in the same invocation request.
4. Return validated structured JSON.
5. Keep prompts, provider credentials, and implementation details private.
6. Support immutable published versions.
7. Allow a production alias to be promoted or rolled back without caller changes.
8. Record usage, latency, status, provider cost, and validation results.
9. Enforce tenant isolation.
10. Protect against runaway inference cost.
11. Remain simple enough for rapid implementation and autonomous maintenance by Goblin.
12. Evolve into a marketplace without requiring a rewrite of the core runtime.
________________


2.2 Secondary Goals
The system should:
* make local development easy,
* make deterministic testing possible,
* minimize provider-specific concepts in public interfaces,
* support additional AI providers later,
* support evaluation datasets later,
* allow synchronous invocation first and asynchronous invocation later,
* expose enough operational data to diagnose model failures,
* keep the core runtime stateless where practical.
________________


2.3 Non-Goals for the MVP
The MVP does not need:
* microservices,
* Kubernetes,
* arbitrary user code execution,
* user-defined containers,
* public marketplace search,
* creator payouts,
* consumer billing,
* multi-step agent workflows,
* RAG,
* vector databases,
* fine-tuning,
* custom model hosting,
* streaming output,
* tool calling,
* model providers beyond OpenAI and Google,
* event-driven workflow orchestration,
* sophisticated routing,
* enterprise SSO,
* public SDK generation.
These should not distort the initial architecture.
________________


3. Architecture Philosophy
3.1 Start as a Modular Monolith
The MVP should be implemented as a modular monolith.
The application has strongly separated internal modules, but most backend functionality is shipped as one deployable service.
This avoids premature distributed-system complexity while preserving boundaries that can later become services.
Logical modules:
Identity
Organizations
Functions
Versions
Deployments
Invocation
Provider Runtime
Usage / Cost
API Keys
Audit
Assets
The key rule is:
Internal module boundaries should be explicit even when deployment boundaries are not.
The runtime should not reach directly into arbitrary database tables owned by other modules. Cross-module behavior should pass through service interfaces.
________________


3.2 Separate Control Plane from Data Plane Conceptually
Even though the MVP may deploy them together, the architecture should distinguish:
Control Plane
Used by creators to:
* create Endpoints,
* edit drafts,
* publish versions,
* manage API keys,
* inspect logs,
* promote versions,
* configure models.
Data Plane
Used by applications to:
* authenticate an invocation,
* resolve an Endpoint,
* validate input,
* invoke a model,
* validate output,
* meter usage,
* return a result.
This separation is important because the data plane may later need:
* independent scaling,
* lower latency,
* stricter availability,
* geographic replication,
* separate rate limits,
* specialized security controls.
________________


4. High-Level Architecture
flowchart LR
    Creator[Creator Browser]
    Client[Consumer Application]

    Web[Web UI]
    API[Platform API]
    Runtime[Invocation Runtime]
    Provider[AI Provider]
    DB[(PostgreSQL)]
    Blob[(Object Storage)]
    Obs[Logs / Metrics / Traces]

    Creator --> Web
    Web --> API
    Client --> Runtime

    API --> DB
    Runtime --> DB
    Runtime --> Blob
    Runtime --> Provider

    API --> Obs
    Runtime --> Obs
For the MVP, API and Runtime may be the same backend process with separate route groups and internal modules.
________________


5. Recommended MVP Technology Stack
The exact libraries are replaceable, but the initial implementation should be opinionated.
5.1 Language
TypeScript
Reasons:
* shared types across web UI and backend,
* strong JSON/schema ecosystem,
* mature HTTP and database tooling,
* easy OpenAPI generation later,
* good fit for model-provider SDKs,
* easy for Goblin to analyze and modify.
________________


5.2 Frontend
Next.js / React
Responsibilities:
* creator dashboard,
* Endpoint editor,
* playground,
* version history,
* API key management,
* invocation logs,
* usage summaries.
The frontend must never receive Provider API Keys.
________________


5.3 Backend
Node.js + Fastify
Responsibilities:
* control-plane REST API,
* invocation REST API,
* authentication,
* authorization,
* schema validation,
* provider execution,
* usage recording.
Fastify is preferred over a framework-heavy backend because the runtime is fundamentally an HTTP/data-processing service.
________________


5.4 Database
PostgreSQL
PostgreSQL is the system of record for:
* users,
* organizations,
* Endpoints,
* versions,
* deployments,
* API key metadata,
* invocation records,
* usage,
* cost,
* audit events.
JSONB may be used for:
* JSON Schemas,
* provider configuration,
* inference parameters,
* normalized provider usage metadata.
Relational columns should still be used for identity, ownership, lifecycle state, timestamps, and lookup keys.
________________


5.5 Object Storage
S3-compatible object storage
Used for:
* temporarily persisted invocation images if required,
* evaluation inputs later,
* large request artifacts,
* future marketplace samples.
The MVP should avoid persisting user images by default unless needed for debugging or explicitly enabled.
________________


5.6 Cache / Redis
Not required initially.
Introduce Redis only when there is a demonstrated requirement such as:
* distributed rate limiting,
* hot function-version cache,
* asynchronous jobs,
* idempotency coordination,
* shared transient state.
A single-process in-memory cache is acceptable for non-authoritative optimization in the MVP.
________________


5.7 Observability
Use:
* structured JSON logs,
* OpenTelemetry-compatible tracing,
* application metrics,
* database-backed invocation history.
Provider request IDs should be captured where available.
Sensitive prompt and user content logging should be disabled by default.
________________


6. Repository Layout
Recommended monorepo:
/
├── apps/
│   ├── web/                 # Creator UI
│   └── server/              # Control plane + invocation API
│
├── packages/
│   ├── domain/              # Domain models and invariants
│   ├── schemas/             # Shared API / JSON schemas
│   ├── runtime/             # Semantic invocation engine
│   ├── providers/           # Model-provider adapters
│   ├── auth/                # AuthN/AuthZ helpers
│   ├── database/            # Migrations and data access
│   ├── observability/       # Logging/tracing/metrics
│   └── test-support/        # Fixtures, fakes, builders
│
├── docs/
│   ├── vision.md
│   ├── architecture.md
│   └── adr/
│
├── infra/
│   ├── docker/
│   └── deploy/
│
└── package.json
The important boundary is that runtime depends on provider interfaces, not directly on a specific provider SDK.
________________


7. Core Domain Model
7.1 Organization
The top-level tenancy boundary.
Organization
  id
  name
  slug
  status
  created_at
  updated_at
Every Endpoint, API key, and invocation belongs to an organization.
Even if the MVP UI initially behaves like a single-user account, the database should establish organization ownership from the beginning.
________________


7.2 User
User
  id
  external_auth_id
  email
  display_name
  created_at
  updated_at
Membership is modeled separately.
________________


7.3 OrganizationMember
OrganizationMember
  organization_id
  user_id
  role
Initial roles:
owner
admin
developer
viewer
The MVP may expose only owner behavior in the UI while retaining the model.
________________


7.4 Endpoint
Represents the stable logical product.
Endpoint
  id
  organization_id
  name
  slug
  description
  visibility
  status
  created_at
  updated_at
Important properties:
* slug is stable within the organization.
* The Endpoint does not directly contain mutable prompt/configuration data.
* Published behavior lives in EndpointVersion.
* The Endpoint owns drafts and published versions.
Example:
Endpoint:
  seed-packet-parser
________________


7.5 EndpointDraft
Mutable creator workspace.
EndpointDraft
  id
  endpoint_id
  revision
  input_schema
  output_schema
  prompt_template
  provider_config
  inference_config
  updated_by
  updated_at
There should be one current draft per Endpoint in the MVP.
Autosave may increment revision for optimistic concurrency.
________________


7.6 EndpointVersion
Immutable snapshot of executable behavior.
EndpointVersion
  id
  endpoint_id
  version_number
  content_hash

  input_schema
  output_schema
  prompt_template
  provider_config
  inference_config

  published_by
  published_at
Once published, these fields must never change.
A new behavior requires a new version.
________________


7.7 DeploymentAlias
Maps a stable channel to a published version.
DeploymentAlias
  id
  endpoint_id
  alias
  endpoint_version_id
  updated_by
  updated_at
Initial alias:
production
Later:
staging
production
canary
The default unversioned invocation resolves production.
________________


7.8 ApiKey
ApiKey
  id
  organization_id
  name
  key_prefix
  key_hash
  scopes
  status
  last_used_at
  created_by
  created_at
  revoked_at
The full key is shown once at creation.
Only a secure hash is stored.
Example scopes:
invoke:endpoint:*
invoke:endpoint:seed-packet-parser
Management API access should not use invocation API keys.
________________


7.9 Invocation
One logical consumer request.
Invocation
  id
  request_id

  caller_organization_id
  endpoint_id
  endpoint_version_id
  api_key_id

  status
  started_at
  completed_at
  duration_ms

  input_bytes
  output_bytes

  provider
  model
  provider_request_id

  prompt_tokens
  input_tokens
  output_tokens
  total_tokens

  estimated_provider_cost
  billable_units

  validation_status
  error_code
Invocation records should contain metadata, not necessarily raw user data.
________________


7.10 InvocationAttempt
A logical invocation may contain more than one provider attempt.
InvocationAttempt
  id
  invocation_id
  attempt_number
  provider
  model
  status
  duration_ms
  usage_json
  estimated_cost
  error_code
  created_at
This allows internal retry behavior while preserving a single external API request.
________________


7.11 Asset
Optional metadata for uploaded images or other binary objects.
Asset
  id
  organization_id
  invocation_id
  storage_key
  media_type
  size_bytes
  sha256
  retention_policy
  expires_at
  created_at
For privacy, the default production policy should be short-lived or non-persistent assets.
________________


7.12 AuditEvent
AuditEvent
  id
  organization_id
  actor_user_id
  action
  resource_type
  resource_id
  metadata
  created_at
Examples:
endpoint.created
endpoint.version.published
endpoint.alias.promoted
api_key.created
api_key.revoked
Audit records should never contain provider secrets.
________________


8. Relational Model
Conceptual relationships:
erDiagram
    ORGANIZATION ||--o{ ORGANIZATION_MEMBER : has
    USER ||--o{ ORGANIZATION_MEMBER : belongs_to

    ORGANIZATION ||--o{ ENDPOINT : owns
    ENDPOINT ||--|| ENDPOINT_DRAFT : has
    ENDPOINT ||--o{ ENDPOINT_VERSION : publishes
    ENDPOINT ||--o{ DEPLOYMENT_ALIAS : exposes

    ENDPOINT_VERSION ||--o{ DEPLOYMENT_ALIAS : targets

    ORGANIZATION ||--o{ API_KEY : owns

    ENDPOINT ||--o{ INVOCATION : receives
    ENDPOINT_VERSION ||--o{ INVOCATION : executes
    API_KEY ||--o{ INVOCATION : authenticates

    INVOCATION ||--o{ INVOCATION_ATTEMPT : contains
    INVOCATION ||--o{ ASSET : may_use

    ORGANIZATION ||--o{ AUDIT_EVENT : records
________________


9. Endpoint Definition
A function version is an immutable executable specification.
Example:
{
  "function": "seed-packet-parser",
  "version": 3,
  "input": {
    "type": "object",
    "properties": {
      "image": {
        "type": "string",
        "contentMediaType": "image/*",
        "x-semantic-type": "image"
      }
    },
    "required": ["image"]
  },
  "implementation": {
    "provider": "openai",
    "model": "configured-vision-model",
    "instructions": "Extract normalized seed packet information...",
    "parameters": {
      "temperature": 0
    }
  },
  "output": {
    "type": "object",
    "properties": {
      "plant": {"type": "string"},
      "variety": {"type": ["string", "null"]},
      "brand": {"type": ["string", "null"]},
      "days_to_maturity": {"type": ["integer", "null"]}
    },
    "required": ["plant"],
    "additionalProperties": false
  }
}
The public consumer must never receive the implementation block.
________________


10. Schema Strategy
10.1 JSON Schema as the Contract
Use JSON Schema as the canonical representation for structured input and output.
Advantages:
* language-neutral,
* machine-readable,
* UI generation is possible,
* OpenAPI mapping is straightforward,
* SDK generation is possible later,
* validation libraries are mature.
________________


10.2 Binary and Multimodal Extensions
Pure JSON Schema does not fully express HTTP multipart behavior.
Use a small platform extension:
{
  "type": "string",
  "contentMediaType": "image/*",
  "x-semantic-type": "image"
}
Supported semantic types in the MVP:
text
image
json
Numeric, boolean, enum, array, and object semantics remain ordinary JSON Schema.
The MVP accepts at most one `x-semantic-type: image` field, and it must be a direct property of the top-level input object. Nested, array-item, root-level, and nullable-branch image fields are rejected at definition validation because multipart invocation exposes one named image attachment.
________________


10.3 Schema Restrictions
The MVP should intentionally support a constrained subset of JSON Schema.
This reduces complexity for:
* UI generation,
* provider translation,
* validation,
* SDK generation later.
Initially support:
* object,
* array,
* string,
* integer,
* number,
* boolean,
* null,
* non-empty arrays of supported `type` values,
* enum,
* required,
* properties,
* items,
* additionalProperties,
* min/max length where practical,
* `maxItems` as a non-negative array bound,
* nullable `anyOf` with exactly one supported typed branch and one null branch.
Each schema node must declare one supported type or a non-empty array of supported types; direct nullable arrays such as `["string", "null"]` remain supported. The constrained nullable `anyOf` wrapper is the only composition form and must contain exactly one supported typed branch and one null branch. Reject unsupported composition and reference keywords at publish time; `anyOf` is not a general-purpose composition facility in the MVP.
________________


11. Control-Plane API
The Parish API is the creator-facing management/control API. All creator-management routes should be under:
/api/control/v1
Example routes:
POST   /api/control/v1/endpoints
GET    /api/control/v1/endpoints
GET    /api/control/v1/endpoints/{endpointId}
PATCH  /api/control/v1/endpoints/{endpointId}

GET    /api/control/v1/endpoints/{endpointId}/draft
PUT    /api/control/v1/endpoints/{endpointId}/draft

POST   /api/control/v1/endpoints/{endpointId}/test
POST   /api/control/v1/endpoints/{endpointId}/publish

GET    /api/control/v1/endpoints/{endpointId}/versions
GET    /api/control/v1/endpoints/{endpointId}/versions/{version}

PUT    /api/control/v1/endpoints/{endpointId}/aliases/production

POST   /api/control/v1/api-keys
GET    /api/control/v1/api-keys
DELETE /api/control/v1/api-keys/{keyId}

GET    /api/control/v1/invocations
GET    /api/control/v1/invocations/{invocationId}
These endpoints are authenticated with creator identity/session authentication, not consumer invocation API keys.
________________


12. Data-Plane Invocation API
12.1 Stable Invocation Route
Recommended public path:
POST /v1/endpoints/{organizationSlug}/{endpointSlug}
Example:
POST /v1/endpoints/acme/seed-packet-parser
This invokes the current production version.
________________


12.2 Explicit Version Route
POST /v1/endpoints/{organizationSlug}/{endpointSlug}/versions/{version}
This allows callers to pin behavior.
________________


12.3 Alias Route
Later:
POST /v1/endpoints/{organizationSlug}/{endpointSlug}/aliases/{alias}
Useful for:
* staging,
* canary,
* alternate production channels.
________________


13. Invocation Payloads
13.1 JSON-Only Function
POST /v1/endpoints/acme/summarize
Authorization: Bearer sfk_...
Content-Type: application/json
{
  "input": {
    "text": "..."
  }
}
________________


13.2 Image Function
The MVP must support a single HTTP request containing both structured fields and an image.
Recommended:
POST /v1/endpoints/acme/seed-packet-parser
Authorization: Bearer sfk_...
Content-Type: multipart/form-data
Parts:
input:
  JSON document containing scalar/structured fields

image:
  binary image
For the seed packet MVP, the API may accept:
image=<binary>
directly when the function has exactly one image field.
________________


13.3 Remote URLs
Supporting remote image URLs may be useful later, but should not be required for the MVP.
Remote URL ingestion introduces:
* SSRF risk,
* redirect handling,
* content-type verification,
* size enforcement,
* private-network access concerns.
If added, it must use a hardened fetcher with outbound network restrictions.
________________


14. Invocation Request Lifecycle
sequenceDiagram
    participant C as Client
    participant R as Invocation API
    participant A as Auth/Quota
    participant V as Version Resolver
    participant E as Runtime Engine
    participant P as Provider Adapter
    participant M as Model Provider
    participant D as Database

    C->>R: POST function + API key + input
    R->>A: Authenticate key
    A-->>R: Caller + scopes + limits

    R->>V: Resolve production/version
    V-->>R: Immutable EndpointVersion

    R->>E: Execute(version, input)
    E->>E: Validate input
    E->>P: Provider-neutral request
    P->>M: Provider-specific request
    M-->>P: Structured response + usage
    P-->>E: Normalized result

    E->>E: Validate output
    E->>D: Record invocation + usage
    E-->>R: Typed result
    R-->>C: JSON response
________________


15. Invocation Runtime Pipeline
The runtime should be implemented as a deterministic pipeline.
1. Create request context
2. Authenticate consumer
3. Check account/key status
4. Check rate limits
5. Resolve function
6. Resolve version or production alias
7. Check invocation authorization
8. Check cost/quota guardrails
9. Parse multipart/JSON request
10. Validate input schema
11. Normalize binary inputs
12. Build provider-neutral invocation
13. Invoke provider adapter
14. Normalize provider response
15. Validate output schema
16. Apply bounded retry/repair policy if configured
17. Record provider usage/cost
18. Record invocation outcome
19. Return normalized response
Each stage should be individually testable.
________________


16. Runtime Interfaces
16.1 Runtime Entry Point
Conceptual TypeScript interface:
interface SemanticRuntime {
  invoke(
    version: EndpointVersionSnapshot,
    input: InvocationInput,
    context: InvocationContext
  ): Promise<InvocationResult>;
}
________________


16.2 Provider-Neutral Request
interface ProviderInvocation {
  model: string;
  instructions: string;
  values: Record<string, unknown>;
  attachments: InvocationAttachment[];
  outputSchema: JsonSchema;
  parameters: InferenceParameters;
}
________________


16.3 Provider Adapter
interface ModelProvider {
  readonly id: string;

  execute(
    invocation: ProviderInvocation,
    context: ProviderExecutionContext
  ): Promise<ProviderResult>;
}
________________


16.4 Provider Result
interface ProviderResult {
  output: unknown;

  usage: {
    inputTokens?: number;
    outputTokens?: number;
    totalTokens?: number;
  };

  providerRequestId?: string;
  rawFinishReason?: string;
}
The runtime should not return provider-native response structures to consumers.
________________


17. Provider Adapter Architecture
The MVP implements two provider adapters: OpenAI and Google.
packages/providers/
  index.ts                 # provider interface, registry, fake provider, cost calculator
  openai/adapter.ts        # OpenAI Responses adapter
  google/adapter.ts        # Gemini Interactions and Vertex generateContent adapters
  Later: additional adapters only after the MVP contract is proven
Provider-specific details must remain inside the adapter.
The rest of the system should understand:
* provider ID,
* model ID,
* normalized token usage,
* normalized errors,
* normalized output.
________________


18. Prompt Template Model
The MVP prompt system should remain intentionally simple.
A function definition contains:
instructions
and optionally a templated user content block.
Example:
You are extracting structured information from a seed packet.

Return only values supported by the packet.
Do not infer a variety if no variety is visible.

Context:
{{context}}
Template substitution should support only named values.
Avoid arbitrary executable template syntax.
Recommended constraints:
* simple {{name}} variables,
* fail on missing required variables,
* no loops,
* no function execution,
* no file system access,
* no network access.
This keeps prompt rendering deterministic and safe.
________________


19. Structured Output
Structured output is a core platform capability, not application logic.
The runtime should:
1. translate the function’s output schema into the provider’s structured-output mechanism,
2. request schema-constrained output,
3. parse the provider response,
4. validate it again independently using the platform validator,
5. return only validated data.
The platform should not trust provider-side schema enforcement alone.
________________


20. Validation Failure Policy
Possible failures:
provider returned invalid JSON
provider returned JSON that violates schema
provider refused
provider response was truncated
provider timed out
MVP policy:
* first attempt uses schema-constrained output,
* one retry may be permitted for clearly recoverable provider/format errors,
* retry policy must be bounded,
* every attempt is recorded,
* the caller still sees one logical invocation,
* no infinite repair loops.
Example normalized error:
{
  "error": {
    "code": "OUTPUT_VALIDATION_FAILED",
    "message": "The function could not produce output matching its contract.",
    "request_id": "req_..."
  }
}
Do not return private prompt text or raw provider errors to the consumer.
________________


21. Endpoint Publishing
Publishing converts mutable draft state into an immutable executable snapshot.
Publish algorithm:
1. Load draft
2. Validate input schema
3. Validate output schema
4. Validate prompt template references
5. Validate provider/model configuration
6. Run static policy checks
7. Canonicalize function definition
8. Compute content hash
9. Allocate next version number
10. Insert immutable EndpointVersion
11. Write audit event
12. Return published version
Publishing should be transactional.
________________


22. Versioning Semantics
22.1 Immutable Versions
If v7 exists, its behavior definition never changes.
This means:
v7 today == v7 six months from now
subject only to unavoidable external provider/model changes.
Where provider APIs permit model revision pinning, the version should capture the pinned model revision.
________________


22.2 Production Alias
The default endpoint resolves:
function -> production alias -> EndpointVersion
Promoting v8:
production: v7 -> v8
does not modify v7 or v8.
Rollback:
production: v8 -> v7
is a metadata update and should be nearly instantaneous.
________________


22.3 Concurrency
Alias promotion should use optimistic concurrency.
The request may include the alias’s current revision/ETag.
This prevents two browser sessions from accidentally overwriting each other’s promotion.
________________


23. Draft Editing
The draft is mutable and should support optimistic concurrency.
Example:
draft_revision = 14
Client submits:
expected_revision = 14
If server is already at revision 15:
409 CONFLICT
This prevents silent prompt/configuration loss.
________________


24. Playground Architecture
The creator playground invokes a draft, not a published endpoint.
Control-plane request:
POST /api/control/v1/endpoints/{id}/test
Test execution should pass through the same runtime pipeline as production wherever possible.
Differences:
* function snapshot comes from draft,
* creator session provides authorization,
* invocation is marked test,
* production quota/billing policy may differ,
* test inputs may optionally be retained for debugging.
The goal is to prevent “works in playground but not in production” divergence.
________________


25. Authentication
25.1 Creator Authentication
Use Firebase Authentication for creator identity/session authentication, with Google enabled as the initial sign-in method.
The creator dashboard uses Firebase-managed Google sign-in and sends an ID token to the control plane. The server verifies token revocation and authorizes the configured owner UID.
Creator sessions are used only for control-plane APIs.
________________


25.2 Consumer Authentication
Consumer applications use API keys.
Example:
sfk_live_abc123...
Key format should include:
* human-recognizable prefix,
* environment hint if needed,
* high-entropy secret.
Database stores:
prefix
cryptographic hash
metadata
not the raw secret.
________________


25.3 API Key Verification
Request:
Authorization: Bearer sfk_live_...
Verification:
1. Validate format
2. Extract non-secret prefix
3. Locate key candidate
4. Constant-time verify secret hash
5. Verify active state
6. Resolve organization
7. Check scopes
8. Update last_used asynchronously or best-effort
________________


26. Authorization
Authorization must be enforced in application services, not just the UI.
Core rule:
Every database query involving tenant-owned data must be scoped by organization.
Examples:
organization A cannot:
  read organization B functions
  publish organization B drafts
  view organization B invocation logs
  revoke organization B keys
Consider database row-level security later, but do not rely on RLS as the only authorization layer.
________________


27. Secret Management
Provider credentials must never be stored in normal application configuration tables as plaintext.
MVP options:
* deployment secret manager,
* encrypted secret store,
* cloud environment secrets.
Initially, provider credentials may be platform-level rather than creator-supplied.
The provider adapter receives credentials through server configuration.
________________


28. Image Handling
The Cottage use case makes image handling part of the MVP architecture.
28.1 Request Limits
Enforce before model invocation:
* maximum request body size,
* maximum image count,
* maximum image bytes,
* allowed MIME types,
* maximum decoded dimensions where feasible.
________________


28.2 Image Validation
Do not trust file extensions.
Validate:
* content signature,
* MIME type,
* size,
* supported format.
Potential MVP formats:
JPEG
PNG
WEBP
HEIC only if decoding/provider path is verified
________________


28.3 Storage Policy
Preferred path:
client
 -> platform request
 -> stream/buffer image
 -> provider
 -> discard
Do not persist image data unless necessary.
If temporary persistence is required:
object storage
+ randomized key
+ encryption
+ short TTL
+ no public access
________________


29. Abuse and Input Safety
The invocation endpoint accepts untrusted Internet traffic.
MVP protections:
* body-size limits,
* MIME validation,
* request timeouts,
* rate limits,
* API-key quotas,
* schema validation,
* provider safety behavior,
* no arbitrary URL fetching by default,
* no arbitrary code execution,
* no caller-controlled provider selection,
* no caller-controlled prompt injection outside declared input fields.
Prompt injection cannot be eliminated, but the impact is limited because Endpoints have no tools or privileged external actions in the MVP.
________________


30. Rate Limiting
Rate-limit dimensions:
API key
organization
function
IP as secondary abuse signal
For a single backend instance, an in-memory limiter is acceptable for the earliest prototype.
Before horizontal scaling, use a shared limiter such as Redis or a gateway-supported rate limiter.
Rate limits should fail before provider invocation.
________________


31. Quotas and Cost Guardrails
The system must prevent uncontrolled provider spend.
31.1 Pre-Invocation Checks
Before inference:
key active?
organization active?
request rate allowed?
daily invocation quota remaining?
input size within limits?
function model allowed?
maximum output configured?
________________


31.2 Hard Limits
MVP hard limits should include:
* per-request maximum output tokens,
* maximum image bytes,
* maximum number of images,
* requests per minute,
* requests per day per organization,
* operator global disable switch.
________________


31.3 Kill Switches
Operators need:
disable all inference
disable one provider
disable one model
disable one organization
disable one function
disable one API key
These checks should be cheap and centralized.
________________


32. Usage and Cost Accounting
Provider usage should be normalized.
Example:
{
  "provider": "openai",
  "model": "model-id",
  "input_tokens": 1450,
  "output_tokens": 210,
  "estimated_cost_usd": 0.0064
}
The cost calculator should be isolated:
interface CostCalculator {
  estimate(
    model: string,
    usage: NormalizedUsage
  ): Money;
}
Do not scatter pricing arithmetic through runtime code.
Provider pricing data can later move into versioned configuration.
________________


33. Marketplace-Ready Accounting
The MVP does not bill consumers, but invocation accounting should preserve enough data for later economics.
Future fields:
provider_cost
platform_cost
consumer_price
creator_revenue
platform_revenue
currency
pricing_version
Do not calculate marketplace payouts in the core invocation transaction.
Instead, write immutable usage events that a later ledger system can consume.
________________


34. Error Model
The public API must normalize errors.
Suggested codes:
AUTHENTICATION_FAILED
AUTHORIZATION_FAILED
ENDPOINT_NOT_FOUND
VERSION_NOT_FOUND
ENDPOINT_DISABLED
INVALID_INPUT
UNSUPPORTED_MEDIA_TYPE
REQUEST_TOO_LARGE
RATE_LIMITED
QUOTA_EXCEEDED
PROVIDER_UNAVAILABLE
PROVIDER_RATE_LIMITED
MODEL_ERROR
OUTPUT_VALIDATION_FAILED
REQUEST_TIMEOUT
INTERNAL_ERROR
Response:
{
  "error": {
    "code": "INVALID_INPUT",
    "message": "Input does not match the function contract.",
    "details": [
      {
        "path": "$.image",
        "message": "image is required"
      }
    ],
    "request_id": "req_01..."
  }
}
Detailed provider errors belong in protected operational logs, not consumer responses.
________________


35. Idempotency
Synchronous AI inference is generally not naturally idempotent.
Support optional:
Idempotency-Key: ...
Behavior:
* same key,
* same organization,
* same function/version,
* same request fingerprint
returns the previously completed result within a bounded retention period.
This is especially valuable once invocations are billable.
It may be deferred until after the first Cottage integration but should be represented in the architecture.
________________


36. Timeouts
Define explicit time budgets.
Example conceptual budgets:
request parse        short
database lookup      short
provider execution   dominant
output validation    short
overall request      bounded
The provider adapter receives a deadline/cancellation signal.
If the caller disconnects, the runtime should cancel provider execution where supported.
________________


37. Database Access Architecture
Avoid direct ORM use from route handlers.
Recommended layering:
HTTP route
  ->
application service
  ->
repository interface
  ->
PostgreSQL implementation
Example:
interface EndpointVersionRepository {
  getPublishedVersion(
    organizationSlug: string,
    endpointSlug: string,
    version: number
  ): Promise<EndpointVersionSnapshot | null>;

  getAliasTarget(
    organizationSlug: string,
    endpointSlug: string,
    alias: string
  ): Promise<EndpointVersionSnapshot | null>;
}
This makes runtime tests independent of PostgreSQL.
________________


38. Transaction Boundaries
Use database transactions for:
Publish
allocate version number
insert version
write audit event
Promote Alias
update alias
write audit event
API Key Creation Metadata
insert key record
write audit event
Invocation execution must not keep a database transaction open during a model call.
Pattern:
create invocation record
COMMIT

call provider

update invocation record
COMMIT
Long-running network calls inside DB transactions should be prohibited.
________________


39. Invocation Persistence Strategy
Suggested lifecycle:
1. Insert Invocation(status=running)
2. Execute provider attempt
3. Insert InvocationAttempt
4. Finalize Invocation(status=succeeded/failed)
If persistence fails after the model returns, the caller should generally still receive the model result if validation succeeded.
Operational accounting must distinguish:
execution success
accounting persistence degraded
This may initially be handled by high-severity logging rather than a durable event queue.
________________


40. Endpoint-Version Caching
Function versions are immutable and therefore ideal cache objects.
Cache key:
endpoint_version:{id}
Production alias resolution is mutable and should have a short TTL.
MVP:
* simple in-process LRU cache,
* invalidate local alias cache after promotions.
Later:
* Redis/shared cache,
* version objects cached aggressively,
* alias records cached briefly.
The database remains authoritative.
________________


41. Deployment Topology
41.1 MVP
Internet
   |
   v
Managed HTTPS / Load Balancer
   |
   +-----------------------+
   |                       |
   v                       v
Web App                Server
                         |
                         +--> Cloud SQL for PostgreSQL
                         +--> Model Provider
Server contains both control plane and data plane.
The MVP request path does not require object storage; uploaded images are buffered for provider execution and discarded.
________________


41.2 Horizontal Scaling
When needed:
Load Balancer
   |
   +--> Server instance A
   +--> Server instance B
   +--> Server instance C
Requirements before doing this:
* no authoritative in-memory state,
* shared rate limiter,
* shared database,
* shared object storage,
* external session store or stateless sessions,
* consistent kill-switch configuration.
________________


42. Future Service Extraction
The first likely extraction is the invocation runtime.
Future:
Creator UI
   |
Control API
   |
PostgreSQL

Consumer Apps
   |
Invocation Gateway
   |
Runtime Workers
   |
Provider Adapters
Do this only when scaling or reliability requirements justify it.
The modular monolith should make extraction mostly a deployment/networking change rather than a domain rewrite.
________________


43. Observability
43.1 Request Correlation
Every incoming request gets:
request_id
Every model execution gets:
invocation_id
attempt_id
These identifiers appear in:
* logs,
* traces,
* database invocation records,
* public errors where appropriate.
________________


43.2 Structured Logs
Log fields:
timestamp
level
request_id
organization_id
endpoint_id
version_id
invocation_id
attempt_id
provider
model
duration_ms
status
error_code
Do not log by default:
* API keys,
* provider secrets,
* full prompts,
* raw images,
* raw user text,
* full model output.
________________


43.3 Metrics
MVP metrics:
invocations_total
invocations_succeeded
invocations_failed
invocation_duration_ms
provider_duration_ms
provider_errors_total
output_validation_failures_total
rate_limit_rejections_total
quota_rejections_total
estimated_provider_cost_total
Breakdowns:
function
version
provider
model
status
Avoid high-cardinality labels such as raw user IDs.
________________


44. Auditability
Creator actions affecting production behavior must be auditable.
Required audit actions:
function.create
endpoint.update_metadata
draft.update
version.publish
alias.promote
api_key.create
api_key.revoke
endpoint.disable
endpoint.enable
An audit event should identify:
who
what
when
resource
before/after identifiers where appropriate
Prompts may be referenced by version/hash rather than duplicated into audit metadata.
________________


45. Privacy
The platform will process user-submitted images and text.
Principles:
1. Retain the minimum data necessary.
2. Make payload retention configurable later.
3. Do not use consumer payloads for unrelated purposes by default.
4. Keep function implementation private from consumers.
5. Separate creator metadata from invocation payload data.
6. Provide an explicit deletion path for retained assets.
7. Avoid storing raw images for normal successful invocations in the MVP.
________________


46. Testing Strategy
The architecture must be designed for automated verification.
46.1 Unit Tests
Test:
* schema validation,
* template rendering,
* cost calculation,
* version resolution,
* authorization,
* API key verification,
* provider error normalization,
* output validation,
* retry policy.
________________


46.2 Provider Adapter Contract Tests
Every provider adapter must pass a shared suite.
Example contract:
accepts text input
accepts image attachment
returns normalized output
returns normalized usage
maps timeout correctly
maps provider rate limit correctly
supports cancellation
never leaks raw provider secrets
The test suite should use fakes for most CI runs.
Optional live-provider tests may run separately.
________________


46.3 Runtime Integration Tests
Use:
* real database,
* fake provider adapter.
Test the complete pipeline:
HTTP request
-> authentication
-> version resolution
-> schema validation
-> fake inference
-> output validation
-> invocation persistence
-> HTTP response
________________


46.4 End-to-End Tests
Critical MVP path:
create function
edit draft
test seed packet
publish v1
create API key
invoke production endpoint
receive valid JSON
publish v2
promote v2
verify unversioned caller gets v2
verify pinned caller still gets v1
rollback production to v1
This should become a mandatory CI scenario.
________________


46.5 Golden Tests
For prompt/runtime behavior, create a small seed-packet fixture set.
Each case contains:
image
expected required fields
allowed alternatives
fields that must not be hallucinated
These are not deterministic unit tests against live models, but they establish the beginning of the later evaluation system.
________________


47. Test Provider
Implement a deterministic fake model provider as a first-class development tool.
Example:
FakeProvider
  exact response fixtures
  injected delay
  injected timeout
  injected malformed JSON
  injected schema violation
  injected provider error
  synthetic token usage
This allows Goblin to develop almost the entire platform without spending inference tokens or depending on external availability.
________________


48. CI/CD
Pipeline:
lint
typecheck
unit tests
integration tests
database migration validation
build
container build
security/static checks
deploy staging
smoke tests
production promotion
Production deployment should not automatically change Endpoint production aliases.
Application deployment and Endpoint-version promotion are separate concerns.
________________


49. Database Migrations
Migrations must:
* be committed,
* run automatically in controlled deployment,
* be forward-compatible during rolling deploys when possible,
* avoid destructive changes without explicit migration steps.
Goblin should not edit production schema manually.
________________


50. Configuration
Environment-level configuration:
DATABASE_URL
OBJECT_STORAGE_*
AUTH_*
PROVIDER_API_KEY
LOG_LEVEL
MAX_REQUEST_BYTES
GLOBAL_INFERENCE_ENABLED
Dynamic operator controls belong in the database/config system, not environment variables alone.
Examples:
provider enabled
model enabled
organization suspended
function disabled
________________


51. Operational Admin Surface
The MVP creator UI does not need a polished operator console, but operators require basic administrative capabilities.
At minimum:
view failing invocations
disable provider
disable function
disable organization
revoke API key
inspect cost totals
inspect provider-error rate
These can initially be CLI/admin-only actions.
________________


52. Security Boundaries
Major trust boundaries:
Internet consumer
   |
   | untrusted
   v
Invocation API
   |
   | authenticated platform code
   v
Runtime
   |
   | privileged provider credentials
   v
Model Provider
And:
Creator browser
   |
   | authenticated but untrusted input
   v
Control API
   |
   v
Endpoint Definition Store
A creator is allowed to control their own prompt but not:
* execute server code,
* read platform secrets,
* access other tenants,
* control arbitrary network destinations.
________________


53. Threat Model Summary
53.1 Stolen Consumer API Key
Risk:
* unauthorized inference spend.
Controls:
* key scopes,
* rate limits,
* quotas,
* revocation,
* last-used visibility,
* optional IP restrictions later.
________________


53.2 Cross-Tenant Access
Risk:
* private prompts or logs leaked.
Controls:
* organization-scoped repositories,
* authorization tests,
* opaque IDs,
* audit logs,
* optional DB RLS later.
________________


53.3 Prompt Exfiltration
Risk:
* consumer tries to induce model to reveal private function prompt.
Controls:
* instruction hierarchy,
* no raw prompt in API response,
* output-schema constraints,
* marketplace creators must assume prompt secrecy is not mathematically guaranteed,
* durable moat should not depend solely on a secret string.
________________


53.4 Provider Credential Leakage
Controls:
* provider keys server-side only,
* redact headers,
* secret manager,
* never serialize keys into function definitions,
* never expose provider raw request objects to consumers.
________________


53.5 Image Bomb / Oversized Upload
Controls:
* HTTP body limits,
* decoded-size checks,
* image validation,
* request timeouts.
________________


53.6 Cost Denial of Service
Controls:
* quota check before inference,
* rate limiting,
* output limits,
* maximum model class,
* global/provider/function kill switches.
________________


54. Marketplace Evolution
The marketplace should be built as additional modules around the runtime, not inside it.
Future modules:
Catalog
Listings
Subscriptions
Pricing
Billing
Ledger
Revenue Share
Payouts
Reviews
Evaluation
Moderation
The invocation runtime should remain concerned with:
who may invoke?
what version?
what input?
what provider execution?
what output?
what usage occurred?
It should not calculate creator payouts inline.
________________


55. Marketplace Domain Extensions
55.1 Listing
MarketplaceListing
  id
  endpoint_id
  publisher_organization_id
  title
  description
  category
  status
  current_public_version
  pricing_plan_id
________________


55.2 Consumer Subscription
EndpointSubscription
  id
  consumer_organization_id
  listing_id
  status
  pricing_plan_id
  created_at
________________


55.3 Pricing Plan
PricingPlan
  id
  listing_id
  currency
  price_per_unit
  free_units
  effective_at
Pricing must be versioned.
Historical calls must always be attributable to the pricing rules active when the call occurred.
________________


56. Usage Event Ledger
Before implementing marketplace billing, introduce append-only usage events.
UsageEvent
  id
  invocation_id
  consumer_organization_id
  publisher_organization_id
  endpoint_id
  version_id
  provider_cost
  billable_quantity
  occurred_at
Later financial processing derives:
consumer charge
creator revenue
platform revenue
from immutable usage events.
Do not mutate historical usage to reflect later pricing changes.
________________


57. Evaluation Architecture
Evaluation should be a separate subsystem layered on top of function versions.
EvaluationDataset
EvaluationCase
EvaluationRun
EvaluationResult
Relationship:
EndpointVersion
   |
   +--> EvaluationRun
          |
          +--> many EvaluationResults
The invocation runtime itself should be reused for eval execution.
This prevents separate “eval behavior” from production behavior.
________________


58. Evaluation Case Model
Conceptual:
EvaluationCase
  input
  expectations
  tags
Expectations may include:
exact field equality
field presence
field absence
numeric range
enum equality
custom judge later
For Cottage:
Input:
  seed packet image

Expectations:
  plant == "Tomato"
  variety == "Brandywine"
  days_to_maturity between 75 and 85
  must_not_invent: germination_temperature if absent
________________


59. Rundale Compatibility
The same runtime should support a Rundale function such as:
npc-response
Input:
{
  "npc": {
    "identity": {},
    "current_state": {},
    "relationships": {}
  },
  "world": {},
  "conversation": [],
  "player_message": "..."
}
Output:
{
  "dialogue": "...",
  "emotion": "uneasy",
  "intent": "deflect",
  "memory_candidates": [],
  "state_changes": []
}
No Rundale-specific runtime code should be necessary.
This is an architectural acceptance test for generality.
________________


60. Cottage Compatibility
Cottage’s production integration should require only:
1. obtain platform API key
2. send seed-packet image
3. decode SeedPacket JSON
Cottage should not contain:
* provider credentials,
* model IDs,
* system prompts,
* retry prompts,
* provider SDKs,
* prompt versions.
A provider/model change should require zero Cottage redeployment.
________________


61. Generated Client Types
Not required for the MVP, but the schema design should allow later generation of:
TypeScript types
Swift Codable structs
Kotlin data classes
Python models
OpenAPI definitions
This could become an important consumer feature.
For Cottage specifically, generating a Swift model/client from the function contract would further reduce integration plumbing.
________________


62. Public API Stability
The public invocation API should change much more slowly than internal provider integrations.
Rules:
* version platform API paths,
* never expose provider-native fields as required consumer contract,
* use additive changes where possible,
* Endpoint versions govern behavior changes,
* platform API versions govern protocol changes.
These are different forms of versioning and must not be conflated.
________________


63. Endpoint Version vs Platform API Version
Example:
POST /v1/endpoints/acme/seed-packet-parser/versions/7
Here:
/v1/      = platform invocation protocol version
versions/7 = semantic-function implementation version
Changing the function from v7 to v8 should not require /v2/.
________________


64. Failure Isolation
A bad Endpoint should not destabilize the platform.
Controls:
* per-request timeout,
* no arbitrary code,
* bounded output,
* bounded retries,
* request isolation,
* Endpoint-level disable switch,
* provider/model allowlist.
A runaway prompt may create bad output or cost, but should not consume unbounded local resources.
________________


65. Performance Targets
MVP performance should prioritize provider latency rather than micro-optimizing local code.
Target local overhead:
authentication
version lookup
validation
logging
should remain small relative to model inference.
Track:
total latency
provider latency
platform overhead = total - provider
This later becomes a marketplace quality metric.
________________


66. Availability Strategy
The MVP has two model-provider dependencies besides its database:
OpenAI and Google
Provider failure should be normalized and observable.
Do not build cross-provider automatic failover initially.
Later, a Endpoint may define:
primary provider/model
fallback provider/model
but that should be an explicit versioned behavior because fallback may affect output quality.
________________


67. Model Lifecycle Risk
Providers may retire models.
A published semantic-function version therefore contains:
configured provider
configured model identifier
publication date
If a provider removes the model:
* mark affected versions degraded/unavailable,
* notify creator,
* require publication of a replacement version,
* do not silently alter immutable function definitions.
If the provider itself aliases model identifiers to changing implementations, record that limitation transparently.
________________


68. Content Hashing
Every published version should have a canonical content hash over:
input schema
output schema
prompt
provider
model
inference parameters
runtime policy fields
Example:
sha256:...
Uses:
* detect duplicate publishes,
* auditability,
* integrity checks,
* future signed manifests,
* reproducibility.
________________


69. Endpoint Manifest
A public-safe manifest may later be exposed:
{
  "name": "Seed Packet Parser",
  "slug": "seed-packet-parser",
  "version": 7,
  "input_schema": {},
  "output_schema": {},
  "description": "Extracts normalized planting data from a seed packet image."
}
It must not include:
* prompt,
* few-shot examples,
* provider credentials,
* private routing strategy.
The model itself may be public or private depending on future marketplace policy.
________________


70. Deployment Environments
Recommended:
local
staging
production
Each environment has separate:
* database,
* provider credentials,
* API keys,
* object storage namespace,
* base URL.
Never use production API keys in local tests.
________________


71. Local Development
A local stack should be launchable with one command.
Example desired behavior:
docker compose up -d
provides:
* PostgreSQL,
* local object storage emulator if required.
Then:
pnpm install --frozen-lockfile
pnpm db:migrate
pnpm db:seed
pnpm dev
runs:
* web,
* server.
A fake provider should make local development possible without external AI credentials.
________________


72. Seed Data
Development seed data should create:
demo organization
demo user
generic image Endpoint draft
published v1
production alias
optional one-time local API key when `SEED_CREATE_API_KEY=true`
This gives local development a stable smoke-test target without embedding Cottage-specific semantics in the runtime.
________________


73. Goblin-Friendly Engineering Constraints
Because Goblin is expected to perform substantial autonomous development, the architecture should optimize for machine-maintainability.
Rules:
1. Prefer explicit code over framework magic.
2. Keep modules small and strongly typed.
3. Keep route handlers thin.
4. Centralize domain invariants.
5. Avoid circular dependencies.
6. Require tests for every bug fix.
7. Keep external integrations behind interfaces.
8. Maintain architecture decision records.
9. Make local setup deterministic.
10. Keep generated code clearly separated.
11. Use static analysis aggressively.
12. Make CI the final authority.
________________


74. Dependency Direction
Preferred dependency graph:
apps/server
   |
   +--> application modules
           |
           +--> domain
           +--> runtime
           +--> repository interfaces

providers
   |
   +--> runtime interfaces

database
   |
   +--> repository interfaces
   +--> domain

web
   |
   +--> public control-plane schemas
The domain layer should not depend on:
* Fastify,
* PostgreSQL,
* provider SDKs,
* Next.js.
________________


75. Architecture Decision Records
Create ADRs for decisions that are costly to reverse.
Current ADR inventory:
ADR-001 TypeScript modular monolith
ADR-002 Constrained JSON Schema contracts
ADR-003 Immutable versions and mutable deployment aliases
ADR-004 Provider-neutral deterministic runtime
ADR-005 Owner identity, tenant isolation, and consumer keys
ADR-006 Metadata-only invocation persistence
ADR-007 Railway deployment topology (superseded)
ADR-008 Google Cloud deployment topology
ADR-009 Firebase owner authentication
ADR-010 Atomic invocation accounting and source integrity
ADR-011 Nullable and bounded-array schema extension
Each ADR should include:
context
decision
alternatives
consequences
status
________________


76. MVP Implementation Sequence
Milestone 0: Skeleton
Build:
* monorepo,
* server,
* web app,
* PostgreSQL,
* migrations,
* CI,
* fake provider.
Acceptance:
health endpoint works
database migrations run
CI passes
________________


Milestone 1: Endpoint Domain
Build:
* organizations,
* Endpoints,
* drafts,
* JSON Schema validation,
* provider/model configuration.
Acceptance:
creator can create/edit a function definition
invalid definitions cannot be saved/published
________________


Milestone 2: Runtime
Build:
* runtime interfaces,
* fake provider adapter,
* input validation,
* output validation,
* invocation records,
* normalized errors.
Acceptance:
published fake function can be invoked through HTTP
________________


Milestone 3: Real Provider
Build:
* OpenAI and Google provider adapters,
* image input,
* structured output,
* token/cost normalization.
Acceptance:
seed packet photo produces valid SeedPacket JSON
________________


Milestone 4: Publishing and Versioning
Build:
* immutable versions,
* version numbering,
* production alias,
* promotion,
* rollback.
Acceptance:
v1 remains callable after v2 is published
production can switch v1 -> v2 -> v1
________________


Milestone 5: Authentication and Guardrails
Build:
* API keys,
* scopes,
* rate limits,
* quotas,
* request-size limits,
* kill switches.
Acceptance:
unauthenticated invocation fails
revoked key fails
quota blocks before inference
________________


Milestone 6: Creator UI
Build:
* function list,
* editor,
* schema editor,
* playground,
* versions,
* API keys,
* invocation log.
Acceptance:
Seed Packet Parser can be created and managed without direct DB/API manipulation
________________


Milestone 7: Cottage Dogfood
Integrate Cottage with the production endpoint.
Acceptance:
Cottage sends photo
platform returns valid SeedPacket
Cottage contains no provider key or prompt
prompt/model changes require no Cottage deployment
This is the MVP completion milestone.
________________


77. Post-MVP Sequence
Recommended next order:
1. production hardening
2. Rundale dogfood
3. eval datasets
4. additional provider support beyond OpenAI and Google
5. organizations/teams polish
6. usage-based consumer billing
7. marketplace listings
8. publisher revenue ledger
9. payouts
10. discovery/reputation
Do not build marketplace presentation before the invocation runtime has real external usage.
________________


78. MVP Acceptance Architecture
The MVP is architecturally successful when all of the following are true:
Generic Runtime
There is no if seedPacket logic in the runtime.
Stable Contract
Cottage depends only on the function API and output schema.
Private Implementation
Cottage cannot retrieve the prompt or platform provider credentials.
Mutable Draft / Immutable Release
Creators can iterate freely, but published versions cannot change.
Provider Isolation
Provider SDK usage is confined to the provider adapter.
Single-Call Image Input
Cottage can submit the image and invoke the function in one HTTP request.
Strong Output Validation
Invalid model output never masquerades as successful typed output.
Observability
Every invocation can be correlated to:
function
version
caller
provider attempt
cost
result
Cost Control
An operator can stop inference before unexpected spend escalates.
Evolution Path
Marketplace accounting, evaluation, and additional providers can be layered on without replacing the core function/version/runtime model.
________________


79. Architectural Invariants
These should be treated as hard rules.
1. Published Endpoint versions are immutable.
2. Provider credentials never leave the server.
3. Consumers never receive private prompts.
4. Every invocation resolves to an explicit Endpoint version.
5. Every successful output passes platform-side schema validation.
6. Every provider attempt is attributable to one logical invocation.
7. Provider network calls never occur inside an open database transaction.
8. Tenant-owned queries are always organization-scoped.
9. Cost guardrails run before inference.
10. The runtime contains no application-specific logic.
11. External provider responses are normalized before leaving the adapter.
12. Endpoint behavior changes create a new version, not mutation.
Violating an invariant requires an explicit architecture decision record.
________________


80. Final Architecture Summary
The MVP should be deliberately simple:
Creator UI
    |
    v
Control API
    |
    +--> PostgreSQL
    |
    +--> Endpoint drafts/versions

Consumer Application
    |
    | API key + input
    v
Invocation API
    |
    +--> resolve immutable function version
    +--> validate input
    +--> invoke provider adapter
    +--> validate output
    +--> record usage/cost
    |
    v
Typed JSON
The durable abstraction is:
Endpoint
    +
Immutable EndpointVersion
    +
Stable Invocation Contract
    +
Provider-Neutral Runtime
Everything else—evaluation, routing, billing, marketplace discovery, revenue sharing, multiple providers—is built around that core.
The platform should feel to the creator like configuring a specialized AI capability and to the consumer like calling an ordinary API.
That is the architecture the MVP must prove.


81. Pinned MVP Implementation Decisions


These decisions are authoritative for the initial Goblin implementation unless superseded by an ADR approved by the project owner.


Deployment and infrastructure


- Hosting: Google Cloud Run in the existing Cottage Google Cloud project, using isolated Parish resources.
- Runtime topology: separate Parish web and Fastify server Cloud Run services backed by a dedicated Cloud SQL PostgreSQL instance.
- The MVP uses the generated Cloud Run web and server hostnames for verification. `app.parish.dev` and `api.parish.dev` remain intended custom domains, but custom DNS is optional follow-up work outside the MVP completion gate.
- Object storage is not required for the first dogfood path. Images should be streamed/buffered through the request to the model provider and discarded. Add S3-compatible storage only when a concrete persistence requirement appears.
- Initial release posture: closed dogfood deployment for the project owner, not public signup.


Creator authentication


- Managed auth provider: Firebase Authentication.
- Supported creator sign-in method: Google.
- Firebase ID tokens authenticate the creator-facing Parish API/control plane.
- Do not build password authentication or account recovery directly in Parish for the MVP.


Model providers


- The MVP supports two providers from the start: OpenAI and Google.
- OpenAI integration uses the current Responses API and must support image input and structured JSON output.
- Google integration uses the current Gemini API, preferring the Interactions API, and must support image input and structured JSON output.
- Both integrations live behind the provider-adapter contract. Endpoint callers never see provider-native request or response shapes.
- No third provider is part of the MVP.


Pinned implementation tooling


- Language: TypeScript.
- Workspace/package manager: pnpm workspaces.
- Frontend: Next.js + React.
- Backend: Node.js + Fastify.
- Database: Cloud SQL for PostgreSQL.
- Database access and migrations: Drizzle ORM + drizzle-kit.
- JSON Schema validation: Ajv.
- Unit/integration test runner: Vitest.
- HTTP integration tests: Fastify inject where possible; real-network tests only where protocol behavior requires them.
- Logging: Fastify/Pino structured logging with explicit secret and payload redaction.
- CI: GitHub Actions.
- Local infrastructure: Docker Compose for PostgreSQL; fake provider enabled by default for deterministic local development.


MVP Endpoint visibility and authorization


- Endpoints are private in the initial dogfood MVP.
- Server-to-server invocation uses Parish API keys scoped to an organization and, where useful, a specific Endpoint.
- API keys are high-entropy random secrets. Store only a cryptographic digest and non-secret prefix; never store or display the full key after creation.
- Public marketplace access, cross-account subscriptions, and public anonymous Endpoints are post-MVP concerns.


Client-safe end-user authentication direction


A long-lived Parish API key must never be embedded in a browser, desktop client, or mobile application intended for distribution. Requiring every developer to build a wrapper backend solely to hide a Parish key would recreate the infrastructure problem Parish exists to remove.


The planned client-safe invocation mode is OIDC/JWT authorization:


1. The Endpoint creator configures one or more trusted OIDC issuers, expected audiences, and optional required scopes/claims for the Endpoint.
2. The developer's browser or mobile application signs its end user in through the developer's existing identity provider using the appropriate OAuth/OIDC flow. Public clients use Authorization Code + PKCE rather than a client secret.
3. The application sends the resulting short-lived bearer token directly to the Parish Endpoint.
4. Parish validates the token signature against the issuer's JWKS, plus issuer, audience, expiry, and configured scopes/claims.
5. If valid, Parish treats the authenticated subject as the end-user principal and invokes the Endpoint. No Parish secret is present in the application.


This design lets developers bring an existing OIDC-compatible identity system rather than forcing all of their end users to create Parish accounts. Provider-specific adapters for common identity systems may be added later, but the contract should remain standard OIDC/JWT validation.


OIDC/JWT Endpoint authorization is not required to complete the owner-only dogfood MVP, but it is a required capability before Parish is presented as a general solution for directly calling private Endpoints from browser/mobile applications.


Applications with no end-user identity cannot securely hide a reusable secret in distributed client code. A later product may support public/anonymous Endpoints with strict quotas, origin restrictions, device/app attestation, and abuse detection, but those are abuse-reduction mechanisms rather than equivalent secret protection.


Cottage contract policy


Do not embed a guessed or copied Cottage SeedPacket schema in this architecture document. When Milestone 7 begins, Goblin must inspect the current Cottage repository on GitHub and derive the integration contract from the actual code at that time. If the code and these documents disagree, the current Cottage code is authoritative for the Cottage-specific adapter/integration, while the generic Parish runtime must remain application-independent.


Dogfood rollout


- First operator/creator: project owner only.
- First real Endpoint: Cottage seed-packet extraction.
- Second dogfood family: Rundale Endpoints using Google and/or OpenAI through the same provider-neutral runtime.
- Do not add marketplace, billing, payouts, or broad signup until the Endpoint authoring, versioning, invocation, and observability loop has proven useful in real Cottage/Rundale use.


MVP implementation authority


Goblin may choose reversible low-level implementation details not specified here, but must record consequential choices as ADRs. It must not broaden product scope, weaken architectural invariants, expose provider credentials or private Endpoint instructions, or add application-specific logic to the generic runtime.


82. Implemented MVP clarifications


- Draft playground executions persist as invocations tied to `endpoint_draft_id`; published API executions persist an explicit `endpoint_version_id`. Exactly one is populated.
- Invocation persistence is metadata-only. Raw input values, images, outputs, creator instructions, credentials, and provider-native payloads are not columns and are redacted from structured logs.
- Supported image content is PNG, JPEG, or WEBP. The server checks magic bytes, declared media type, byte limits, and decoded pixel dimensions before provider execution.
- The Google adapter uses the Interactions API for Gemini API-key deployments and stateless `generateContent` for Vertex AI identity deployments, where the selected model does not expose Interactions. Because neither selected generation configuration exposes the public temperature setting consistently, Google definitions containing temperature are rejected instead of silently ignoring creator configuration.
- Estimated costs use explicit per-model price configuration. Live mode fails closed when any allowed model lacks a configured price.
- The initial single server replica uses an in-memory rate gate plus persistent daily organization quotas. A shared limiter remains a prerequisite for horizontal server scaling.
- Operational global, provider, and model switches are managed through the checked-in operator CLI; organization and Endpoint switches remain database-backed domain controls.
- Google Cloud deployment uses separate server and web Dockerfiles from the shared monorepo root, Cloud Build configuration, Artifact Registry, an explicit migration job, and a dedicated least-privilege runtime identity. See `docs/deployment.md` and ADR 008 for external configuration and verification.
