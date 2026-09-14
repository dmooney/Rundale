# ADR 004: Provider-neutral deterministic runtime

- Status: Accepted
- Date: 2026-08-25

## Decision

The semantic runtime depends on a provider interface. OpenAI Responses and Google Gemini transports are confined to adapters that normalize image input, structured output, usage, request IDs, and errors. Gemini API-key deployments use Interactions; Vertex AI identity deployments use stateless `generateContent` because the selected Vertex model does not expose Interactions. Provider calls happen after invocation metadata is committed and never inside database transactions. Retry is bounded to zero or one repeat of a retryable execution.

## Consequences

Consumers never observe provider-native structures or credentials. Provider changes do not alter Endpoint routes, schemas, or clients. Google temperature is rejected at definition validation because the selected Interactions adapter does not expose that setting.
