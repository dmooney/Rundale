Parish Endpoints: Product Vision
Status: Draft
Working name / domain: Parish / parish.dev
Initial implementation system: Goblin
1. Vision
Make proprietary AI behavior publishable as a hosted API endpoint without requiring the creator to build, host, scale, secure, or operate an API service.
A creator should be able to define:
* what goes in,
* what model to use,
* what instructions govern the behavior,
* what structured result must come out,
and immediately receive a stable, hosted endpoint.
The long-term product extends this model into a marketplace where creators can publish high-quality AI capabilities, developers can integrate them with a single API call, and the platform handles execution, authentication, metering, billing, and revenue sharing.
The product is not a prompt marketplace.
It is a marketplace and runtime for Endpoints: versioned, testable, machine-consumable capabilities whose internal implementation may consist of prompts, model selection, examples, schemas, validation, routing, or other inference logic.
________________


2. The Core Idea
Traditional APIs expose deterministic software functions:
input -> code -> output
Parish Endpoints expose useful AI behavior:
typed input
   ->
private AI implementation
   ->
typed output
For example:
SeedPacketParser

Input:
  image

Output:
  SeedPacket

Behavior:
  Examine a photograph of a seed packet and return normalized,
  structured planting information.
The caller does not need to know:
* which model is used,
* how the prompt is written,
* whether few-shot examples are included,
* whether the implementation changed,
* how retries or validation are performed,
* which provider executes the request.
The caller depends only on the API contract.
________________


2.1 Parish Naming and Product Boundary
Parish is the working umbrella name and parish.dev is the working domain.
Parish may ultimately encompass multiple developer tools, including:
* Parish Engine — the Rundale-derived game/simulation engine.
* Parish Endpoints — the hosted AI API product described in this document.
These remain separate runtime boundaries and deployables even when the
Endpoints workspace is maintained inside the Rundale repository.
Within this product, Endpoint is the working first-class noun.
An Endpoint is not a Lambda-style serverless function. A creator does not upload arbitrary code and does not define event-triggered compute. Instead, the creator defines a hosted HTTP API behavior consisting of:
* an HTTP-facing input contract,
* private or proprietary canned instructions,
* a selected LLM/model,
* inference settings,
* an output contract,
* versioned publication state.
Parish hosts the HTTP interface and performs the managed LLM invocation behind it.
Terminology:
Term
	Meaning
	Endpoint
	The hosted AI API object a creator defines and publishes
	Endpoint Version
	An immutable published implementation/configuration
	Endpoint Definition
	Model, private instructions, schemas, and inference settings
	Parish API
	The management/control API creators use to create and manage Endpoints
	Endpoint URL
	The callable HTTP URL an application invokes
	Parish Engine
	Separate Rundale-derived game/simulation tooling under the Parish umbrella
	The implementation behind an Endpoint may become more sophisticated over time, but the consumer contract remains an API endpoint rather than executable user code.
________________


3. Problem
Building a small AI-powered feature currently requires too much infrastructure.
A developer who wants to turn a photograph into structured data often has to create:
1. a backend service,
2. model-provider integration,
3. authentication,
4. secret management,
5. prompt storage,
6. structured-output handling,
7. retries and validation,
8. deployment,
9. scaling,
10. request logging,
11. usage metering,
12. billing or quota controls.
For many AI features, almost all of that infrastructure exists only to wrap a single model call.
This is disproportionate to the actual application logic.
Example: Cottage
Cottage needs a capability that does one thing:
photograph of seed packet -> SeedPacket JSON
The application should not need a custom backend whose primary purpose is to hide an API key and store a prompt.
Example: Rundale
Rundale may need many semantic capabilities:
NPC state + player message -> NPC response
recent experiences -> compressed memory
world state -> candidate event
event + NPC context -> interpreted rumor
location state -> scene description
These are distinct application functions, but they share the same infrastructure problem.
________________


4. Product Thesis
AI capabilities should be deployable at a higher abstraction level than containers, serverless functions, or model endpoints.
The unit of publication should be an Endpoint.
A Parish Endpoint has:
* a stable identifier,
* an input contract,
* an output contract,
* a private implementation,
* a version,
* a runtime configuration,
* observable quality and usage.
Creating one should feel closer to configuring a custom GPT than deploying a cloud service.
Calling one should feel like calling Stripe, Twilio, or any ordinary REST API.
________________


5. Target Experience
A creator opens the product and creates an Endpoint.
Name
  Seed Packet Parser

Input
  image: image

Model
  GPT-5.x vision-capable model

Prompt
  Extract all useful planting information from this seed packet...

Output
  SeedPacket JSON Schema

[ Test ]

[ Publish ]
The platform returns:
POST /v1/endpoints/parish-demo/seed-packet-parser
Authorization: Bearer <caller-api-key>
Content-Type: multipart/form-data
The caller sends an image.
The platform returns:
{
  "plant": "Tomato",
  "variety": "Brandywine",
  "brand": "Burpee",
  "days_to_maturity": 80,
  "sun": "full_sun",
  "planting_depth_inches": 0.25,
  "spacing_inches": 24
}
No application-specific API server is required.
________________


6. Long-Term Marketplace Vision
Once Endpoints can be created, hosted, versioned, and invoked, they can become products.
A creator could publish:
Seed Packet Parser
image -> SeedPacket
$0.02 / call
99.2% evaluation score
Other developers could discover the capability, test it, obtain credentials, and integrate it.
The platform would:
* authenticate callers,
* meter requests,
* execute the function,
* pay model-provider costs,
* charge the caller,
* retain a platform fee,
* pay the creator,
* track versions and quality.
The creator’s implementation remains private.
The product being sold is not the prompt. It is the behavioral contract and demonstrated capability.
________________


7. Users
Endpoint Creators
Developers, domain experts, and AI practitioners who can create reliable AI behavior but do not want to operate an API business.
They want to:
* define a capability,
* test it,
* improve it,
* publish it,
* get paid when it is used.
Endpoint Consumers
Application developers who want to add AI capabilities without becoming prompt engineers or AI infrastructure operators.
They want:
* one stable API,
* predictable input/output,
* reliable behavior,
* transparent pricing,
* easy integration.
Internal Application Teams
Teams may also use the platform privately without ever publishing functions publicly.
For them, the product is a managed internal AI-function layer.
Cottage and Rundale are initial examples.
________________


8. Product Principles
APIs, Not Chatbots
The primary interface is machine-to-machine invocation.
The product is not centered on conversations, assistants, or end-user chat.
Typed Contracts
Inputs and outputs should be explicit.
JSON Schema should be a first-class concept.
Implementation Is Private
Consumers depend on behavior, not prompts.
Creators should be able to change model, prompt, examples, or internal strategy without exposing implementation details.
Version Everything
A published Endpoint version should be immutable.
New behavior should produce a new version that can be tested before promotion.
Model-Agnostic Runtime
Applications should not depend directly on OpenAI, Anthropic, Google, or another provider.
The platform owns provider integration.
Quality Is Part of the Product
An Endpoint should eventually be able to carry evaluation results, latency statistics, reliability metrics, and version history.
One Call Should Be Enough
Common multimodal use cases should not require separate upload, preprocessing, and inference calls.
For example, a caller should be able to submit an image directly with the invocation request.
________________


MVP
9. MVP Goal
Prove that a developer can define an AI capability in a web interface and immediately consume it from a real application through a hosted API without writing or deploying backend inference code.
The MVP succeeds when Cottage can replace its seed-packet AI integration with an Endpoint hosted entirely by Parish.
The marketplace is not required for the MVP.
________________


10. MVP Primary Use Case
The first production-quality Endpoint will be:
Seed Packet Parser

Input:
  image

Output:
  Cottage SeedPacket JSON
Cottage should be able to invoke the Endpoint with one HTTP request and receive validated structured data.
This provides a concrete dogfooding target while keeping the platform itself general-purpose.
________________


11. MVP Creator Workflow
A creator must be able to:
1. Create an account.
2. Create an Endpoint.
3. Give it a name and slug.
4. Define an input schema.
5. Define an output JSON Schema.
6. Select a supported model.
7. enter a system/instruction prompt.
8. Configure basic inference settings.
9. Test the function interactively.
10. Save revisions.
11. Publish a version.
12. Create an API key.
13. Invoke the published function from an external application.
14. View basic request logs and usage.
________________


12. MVP Runtime
Invocation flow:
client
  |
  | request + API key
  v
API gateway
  |
  +-- authenticate
  +-- resolve function/version
  +-- validate input
  |
  v
semantic runtime
  |
  +-- construct provider request
  +-- inject prompt
  +-- attach image/text inputs
  +-- request structured output
  |
  v
model provider
  |
  v
semantic runtime
  |
  +-- validate output
  +-- record usage/cost
  +-- normalize errors
  |
  v
client
________________


13. MVP Endpoint Definition
A minimal internal representation could resemble:
{
  "id": "fn_123",
  "slug": "seed-packet-parser",
  "version": 3,
  "model": "provider/model-id",
  "instructions": "Extract normalized seed packet information...",
  "input_schema": {
    "type": "object",
    "properties": {
      "image": {
        "type": "image"
      }
    },
    "required": ["image"]
  },
  "output_schema": {
    "type": "object",
    "properties": {
      "plant": {"type": "string"},
      "variety": {"type": ["string", "null"]},
      "days_to_maturity": {"type": ["integer", "null"]}
    },
    "required": ["plant"]
  }
}
The exact representation is an implementation detail.
The important requirement is that the runtime can interpret the same function definition generically.
________________


14. MVP API
A minimal public API might expose:
POST /v1/endpoints/{organizationSlug}/{endpointSlug}
POST /v1/endpoints/{organizationSlug}/{endpointSlug}/versions/{version}
Example:
POST /v1/endpoints/parish-demo/seed-packet-parser
Authorization: Bearer sfk_live_...
The default endpoint invokes the currently promoted production version.
Explicit version invocation allows callers to pin behavior.
Response Requirements
Successful responses should contain only the declared output structure plus minimal platform metadata if needed.
Errors should be normalized across model providers.
Example categories:
* authentication failure,
* invalid input,
* function not found,
* model execution failure,
* output validation failure,
* rate limit exceeded,
* quota exceeded.
________________


15. MVP Model Support
Start with two model providers: OpenAI and Google.
Implement the provider-adapter abstraction in the MVP with OpenAI and Google, because Cottage and Rundale require both. Do not add additional providers before the core product works.
The architecture must avoid hard-coding provider-specific concepts into the public API. The MVP implementation supports both OpenAI and Google behind the same provider-adapter boundary.
Additional providers can be introduced after the runtime contract is proven.
________________


16. MVP Input Types
Required:
* text,
* number,
* boolean,
* enums,
* structured JSON,
* image.
Images are essential because the first real consumer is Cottage.
The invocation API should allow an image to be supplied in the same request as the function call.
________________


17. MVP Output Types
The MVP should optimize for structured JSON.
The primary contract is:
arbitrary supported input -> validated JSON
Free-form text may be supported, but it is not the differentiating feature.
JSON Schema validation is part of the runtime, not an optional caller responsibility.
________________


18. MVP Versioning
Each saved edit may create a draft revision.
Publishing creates an immutable version.
Example:
seed-packet-parser
  v1
  v2
  v3 <- production
  draft
The creator must be able to:
* test a draft,
* publish it,
* promote a published version,
* roll production back to an earlier version.
Existing pinned callers must continue to receive the version they requested.
________________


19. MVP Observability
For each invocation, record:
* timestamp,
* function,
* version,
* caller/API key,
* model,
* latency,
* success/failure,
* provider usage,
* estimated provider cost,
* validation result.
The MVP UI should expose a simple request log and aggregate request count.
Full observability tooling is not required initially.
________________


20. MVP Security Requirements
Security cannot be deferred simply because the product is a prototype.
The MVP must include:
* server-side provider credentials,
* hashed platform API keys,
* tenant isolation,
* authorization checks on every function-management operation,
* request-size limits,
* image-size limits,
* rate limiting,
* secure logging that avoids accidental secret exposure.
Creator prompts and function implementations must never be returned to API consumers.
________________


21. MVP Cost Controls
AI inference creates an unusual failure mode: software bugs and abuse can create direct variable cost.
The MVP should therefore include:
* per-account request limits,
* per-key rate limits,
* maximum input sizes,
* model allowlists,
* token/output limits,
* request cost recording,
* an operator kill switch.
Later versions can add budgets and automatic spend caps.
________________


22. MVP UI
The MVP requires only a small application.
Endpoints
A list of the creator’s Endpoints.
Endpoint Editor
Fields for:
* name,
* slug,
* input definition,
* output JSON Schema,
* model,
* prompt,
* inference settings.
Playground
Allow test inputs, including image upload, and display:
* raw request,
* structured result,
* validation errors,
* latency,
* estimated model cost.
Versions
Show published versions and allow promotion/rollback.
API Keys
Create and revoke keys.
Usage
Show recent requests and basic totals.
________________


23. MVP Non-Goals
The following should explicitly not be required for the first release:
* public marketplace,
* creator payouts,
* consumer billing,
* ratings or reviews,
* search/discovery,
* arbitrary Python execution,
* custom containers,
* user-provided model weights,
* multi-step workflows,
* autonomous agents,
* tool calling,
* RAG pipelines,
* vector databases,
* fine-tuning,
* complex model routing,
* model providers beyond OpenAI and Google,
* enterprise SSO,
* custom domains,
* full OpenAPI generation,
* sophisticated eval infrastructure.
These may become important later, but they are not necessary to validate the core product.
________________


24. MVP Success Criteria
The MVP is successful if:
1. An Endpoint can be created without writing backend serving code.
2. Publishing it produces an immediately callable hosted endpoint.
3. Cottage can send a seed-packet image in one request.
4. Cottage receives valid SeedPacket JSON.
5. The prompt and model can be changed without modifying or redeploying Cottage.
6. A new Endpoint version can be tested before becoming production.
7. Production can be rolled back to an earlier version.
8. Provider credentials never appear in Cottage.
9. Invocation cost and usage can be observed.
10. The runtime contains no Cottage-specific code.
The final criterion is particularly important.
If the platform must understand what a seed packet is, the abstraction has failed.
________________


Beyond the MVP
25. Phase 2: Production Platform
After the core abstraction is proven:
* multiple provider support,
* better authentication,
* teams and organizations,
* function aliases,
* quotas and budgets,
* schema-generated SDKs,
* streaming where appropriate,
* webhooks,
* better logs,
* prompt diffing,
* latency and cost analytics,
* environment separation,
* staging and production promotion.
Rundale can serve as the second major dogfood application because it exercises multiple Endpoints and structured state-based inference.
________________


26. Phase 3: Evaluation Layer
Creators should be able to define evaluation datasets:
input -> expected properties
New function versions could be automatically tested before publication.
The platform could expose:
* correctness score,
* schema success rate,
* regression detection,
* latency,
* estimated cost,
* version comparisons.
This turns an Endpoint from an opaque prompt configuration into a measurable API product.
________________


27. Phase 4: Marketplace
Once the runtime is reliable, creators may publish selected Endpoints publicly.
Marketplace listings could include:
* function name,
* description,
* input schema,
* output schema,
* examples,
* price,
* evaluation results,
* usage volume,
* latency,
* publisher identity,
* version history.
Consumers should be able to test a function before integrating it.
________________


28. Marketplace Economics
A simple future model:
consumer price per call
        -
underlying inference cost
        =
gross margin

gross margin
        ->
creator share
platform share
The platform could also permit creators to specify a markup or fixed per-call price subject to minimum pricing rules.
The caller should have one billing relationship with the platform regardless of which underlying model or creator function is used.
The creator should not need to establish a billing relationship with every consumer.
________________


29. Marketplace Flywheel
A functioning marketplace could create a useful specialization dynamic:
1. A creator becomes exceptionally good at one semantic task.
2. They publish that capability.
3. Many applications consume it.
4. Usage creates revenue and real-world performance data.
5. The creator improves the function.
6. Consumers receive improvements without rebuilding the capability internally.
7. Strong Endpoints attract more usage.
Examples:
image -> baseball card metadata
image -> antique identification
PDF -> normalized resume
photo -> property damage estimate
product image -> ecommerce attributes
clinical text -> structured coding suggestions
game state -> NPC cognition result
The value resides in accumulated behavioral quality, not merely access to a foundation model.
________________


30. Competitive Position
Existing products generally solve only part of the problem.
Model APIs
They provide inference but leave application behavior and deployment plumbing to the developer.
Prompt Management Platforms
They store and version prompts but generally do not create a monetizable public API product.
Serverless Platforms
They host code but still require developers to write and operate software.
API Marketplaces
They provide discovery and billing but typically require publishers to bring an already-hosted API.
Model Marketplaces
They distribute models but make the model, rather than the semantic capability, the unit of productization.
This product combines:
GPT-like authoring
+ managed inference
+ typed APIs
+ serverless deployment
+ evaluation
+ marketplace distribution
+ usage-based creator revenue
________________


31. Strategic Differentiation
The most important abstraction is:
The Endpoint, not the model or prompt, is the product.
Foundation models will continue changing rapidly.
A good consumer API should not.
A developer integrating:
seed-packet-parser
should not care whether version 18 uses:
* a different OpenAI model,
* Anthropic,
* Gemini,
* an open model,
* several models,
* deterministic preprocessing,
* validation and repair,
* or some future inference technology.
The function contract survives model churn.
That is the durable layer.
________________


32. Risks
Foundation Providers Add This Directly
OpenAI, Anthropic, Google, or another provider could introduce hosted semantic-function marketplaces.
Mitigation: move quickly, stay provider-independent, and build value around quality measurement, marketplace data, and cross-provider execution.
Endpoints Are Too Easy to Copy
Simple prompts may be commoditized.
Mitigation: emphasize evaluation quality, accumulated datasets, version history, reliability, routing, and implementation secrecy.
Inference Economics Are Thin
Marketplace margins may be insufficient for trivial Endpoints.
Mitigation: allow creators to charge for capability value rather than token markup alone.
Untrusted Content
Images and text may contain malicious or policy-sensitive content.
Mitigation: input constraints, provider safety systems, abuse controls, reporting, and marketplace moderation.
Cost Abuse
Stolen credentials or runaway applications can create rapid expense.
Mitigation: quotas, rate limits, budgets, anomaly detection, and hard account caps.
Quality Is Difficult to Compare
Two APIs claiming the same task may behave very differently.
Mitigation: first-class evaluations and reproducible benchmarks should become a major platform feature.
________________


33. Initial Development Strategy
Build the platform through dogfooding.
First Consumer: Cottage
Implement:
image -> SeedPacket JSON
This validates multimodal input, structured output, schema enforcement, authentication, versioning, and remote prompt management.
Second Consumer: Rundale
Implement several Endpoints such as:
NPC state + message -> structured NPC response
experiences -> memory
world state -> event
This validates multiple Endpoints, more complex schemas, version management, latency sensitivity, and behavioral iteration.
If one generic runtime supports both applications cleanly, the fundamental abstraction has strong evidence behind it.
________________


34. Ultimate Product
The mature product should make publishing an AI capability as easy as creating a custom GPT, while making consuming that capability as easy as calling Stripe.
A creator should be able to turn expertise into a callable Endpoint.
A developer should be able to integrate that expertise without understanding the implementation.
The platform should handle everything between those two parties.
Define behavior.
Test it.
Publish it.
Call it.
Measure it.
Improve it.
Get paid for it.
