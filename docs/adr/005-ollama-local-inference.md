# ADR-005: Ollama Local Inference

> Back to [ADR Index](README.md) | [Docs Index](../index.md)

## Status

Accepted (2026-03-18)

## Context

Rundale's core innovation is LLM-driven NPC cognition and natural language input parsing. Every NPC interaction, nearby NPC-to-NPC conversation, and player command requires inference. This creates several requirements:

- **Privacy**: Player interactions and game content must stay local. No data leaves the machine.
- **Cost**: With potentially hundreds of inference calls per play session, cloud API costs would be prohibitive.
- **Offline play**: The game must work without an internet connection.
- **Throughput**: The cognitive LOD system (ADR-002) requires different model sizes for different tiers, with enough throughput to keep gameplay responsive.
- **Hardware**: The target system has an RX 9070 16GB GPU (AMD, requiring ROCm) and an Intel i9-13900KS CPU.

## Decision

Use **Ollama** as the local inference server, running on `localhost:11434` and accessed via its REST API using the `reqwest` HTTP client.

**Model allocation by size class:**

| Tier | Size class | Purpose |
|------|-----------|---------|
| Tier 1 (immediate) | ~9B dialogue-tuned | Full NPC dialogue, rich interaction |
| Tier 2 (nearby) | ~9B JSON-tuned | Lighter NPC-to-NPC interaction |
| Tier 3 (distant) | ~9B JSON-tuned | Batch simulation of many NPCs |
| Player input parsing | ~3B JSON / function-calling | Natural language intent detection |

Specific picks are maintained in [docs/design/inference-pipeline.md](../design/inference-pipeline.md#recommended-models-april-2026) and refreshed as the open-model ecosystem evolves. This ADR was originally accepted with Qwen3 14B as the Tier 1 reference model; as of April 2026 the ecosystem has converged on 9B dialogue models (Gemma 4 9B, Qwen 3.5 9B) as the new Tier 1 baseline.

**Inference pipeline:**

```
Simulation Threads -> Inference Queue (Tokio mpsc) -> Inference Worker -> Ollama REST API -> Response Router -> World State Update
```

- A Tokio mpsc channel serves as the inference queue
- A dedicated async task pulls requests, sends them to Ollama, and routes responses back
- Batch requests where possible for Tier 2/3 NPCs
- Explicit timeouts on all reqwest HTTP requests

**Expected throughput:**

- 9B-class model on RX 9070 (q4 quantization): ~40-60 tokens/sec
- At ~100-150 tokens per NPC response: ~3-6 NPC "thoughts" per second

See inference-pipeline.md for current throughput ranges per model and cloud-provider comparisons.

## Consequences

**Positive:**

- Zero cloud dependency: no API keys, no billing, no rate limits
- Complete privacy: all data stays on the local machine
- Works fully offline after initial model download
- Ollama handles model loading, GPU memory management, and request queuing
- REST API is simple to integrate via reqwest
- Multiple model sizes can be loaded for different tiers
- Active open-source project with broad model support

**Negative:**

- Hardware-bound throughput: ~40-60 tokens/sec on 9B is a hard ceiling (local-only; cloud paths are not subject to this ceiling)
- ROCm setup on AMD GPUs can be complex and fragile
- Ollama must be running as a separate process before the game starts
- Model switching between tiers may incur loading latency if GPU memory is constrained
- Ollama's REST API adds HTTP overhead compared to direct model integration
- Dependent on Ollama project maintenance and compatibility

## Alternatives Considered

- **llama.cpp direct integration**: Would eliminate the HTTP overhead and Ollama dependency, but significantly increases integration complexity. Would need to handle model loading, GPU memory management, and batching directly in Rust. Tighter coupling makes model switching harder.
- **Cloud APIs (OpenAI, Anthropic, etc.)**: Low integration effort but introduces latency, ongoing cost (potentially significant at hundreds of calls per session), privacy concerns, and requires internet connectivity. Fundamentally incompatible with the offline-first design goal.
- **No LLM (traditional game AI)**: Eliminates inference complexity entirely but loses the core innovation. NPC behavior would be limited to state machines and scripted responses, producing the same predictable interactions as traditional text adventures.
- **GGML/GGUF direct loading in Rust**: Possible via `llm` or `candle` crates, but these are less mature than Ollama for production use and would require managing the full inference stack.

## Related

- [docs/design/inference-pipeline.md](../design/inference-pipeline.md)
- [ADR-002: Cognitive Level-of-Detail Tiers](002-cognitive-lod-tiers.md)
- [ADR-006: Natural Language Input](006-natural-language-input.md)
- [ADR-008: Structured JSON LLM Output](008-structured-json-llm-output.md)
