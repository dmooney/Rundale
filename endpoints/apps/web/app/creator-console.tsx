"use client";

import {
  GoogleAuthProvider,
  onAuthStateChanged,
  signInWithPopup,
  signOut,
  type User,
} from "firebase/auth";
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { getFirebaseAuth } from "./firebase";

type Endpoint = { id: string; name: string; slug: string; description: string; status: string };
type Provider = "fake" | "openai" | "google";
type ModelOption = { provider: Provider; model: string };
type Definition = {
  inputSchema: Record<string, unknown>;
  outputSchema: Record<string, unknown>;
  instructions: string;
  providerConfig: ModelOption;
  inferenceConfig: { maxOutputTokens: number; retryCount: 0 | 1; temperature?: number };
};
type Draft = Definition & { id: string; endpointId: string; revision: number };
type Version = {
  id: string;
  version: number;
  contentHash: string;
  publishedAt: string;
  isProduction: boolean;
  productionAliasRevision: number | null;
};
type ApiKey = {
  id: string;
  name: string;
  keyPrefix: string;
  scopes: string[];
  status: string;
};
type Invocation = {
  id: string;
  requestId: string;
  endpointId: string;
  endpointVersionNumber?: number | null;
  endpointDraftId: string | null;
  status: string;
  isTest: boolean;
  provider: string;
  model: string;
  durationMs: number | null;
  validationStatus: string;
  errorCode: string | null;
  startedAt: string;
};
type Usage = {
  invocations: number;
  succeeded: number;
  failed: number;
  totalTokens: number;
  estimatedProviderCost: string;
};
type JsonField = "inputSchema" | "outputSchema";

const starterInputSchema = {
  type: "object",
  properties: {
    image: { type: "string", "x-semantic-type": "image" },
    note: { type: "string" },
  },
  required: ["image"],
  additionalProperties: false,
};
const starterOutputSchema = {
  type: "object",
  properties: { result: { type: "string" } },
  required: ["result"],
  additionalProperties: false,
};

function starterDefinition(providerConfig: ModelOption): Definition {
  return {
    inputSchema: starterInputSchema,
    outputSchema: starterOutputSchema,
    instructions: "Inspect the input and return a concise, schema-valid result.",
    providerConfig,
    inferenceConfig: { maxOutputTokens: 512, retryCount: 0 },
  };
}

function modelKey(model: ModelOption): string {
  return `${model.provider}/${model.model}`;
}

class ApiError extends Error {
  constructor(
    message: string,
    readonly code: string,
  ) {
    super(message);
  }
}

function parseJsonObject(text: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return null;
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

function definitionFromDraft(draft: Draft): Definition {
  return {
    inputSchema: draft.inputSchema,
    outputSchema: draft.outputSchema,
    instructions: draft.instructions,
    providerConfig: draft.providerConfig,
    inferenceConfig: draft.inferenceConfig,
  };
}

function definitionFingerprint(definition: Definition): string {
  return JSON.stringify(definition);
}

function invocationVersionLabel(invocation: Invocation): string {
  return typeof invocation.endpointVersionNumber === "number"
    ? `v${invocation.endpointVersionNumber}`
    : invocation.endpointDraftId === null
      ? "version"
      : "draft";
}

function formatStartedAt(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function JsonEditor({
  label,
  text,
  onChange,
  invalid = false,
}: {
  label: string;
  text: string;
  onChange(text: string): void;
  invalid?: boolean;
}) {
  return (
    <label>
      {label}
      <textarea
        className="code"
        aria-invalid={invalid}
        value={text}
        onChange={(event) => onChange(event.target.value)}
      />
      {invalid && <small role="alert">Enter a valid JSON object before saving.</small>}
    </label>
  );
}

function ConsoleWithToken({
  getToken,
  ownerEmail,
  onSignOut,
}: {
  getToken(): Promise<string | null>;
  ownerEmail?: string;
  onSignOut?(): void;
}) {
  const apiBase = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001";
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [models, setModels] = useState<ModelOption[]>([]);
  const [newModelKey, setNewModelKey] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [savedDefinitionFingerprint, setSavedDefinitionFingerprint] = useState("");
  const [versions, setVersions] = useState<Version[]>([]);
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [invocations, setInvocations] = useState<Invocation[]>([]);
  const [usage, setUsage] = useState<Usage>({
    invocations: 0,
    succeeded: 0,
    failed: 0,
    totalTokens: 0,
    estimatedProviderCost: "0.000000",
  });
  const [inputText, setInputText] = useState('{"note":"owner playground"}');
  const [image, setImage] = useState<File | null>(null);
  const [result, setResult] = useState<unknown>(null);
  const [secret, setSecret] = useState("");
  const [notice, setNotice] = useState("Loading owner workspace…");
  const [schemaText, setSchemaText] = useState<Record<JsonField, string>>(() => ({
    inputSchema: JSON.stringify(starterInputSchema, null, 2),
    outputSchema: JSON.stringify(starterOutputSchema, null, 2),
  }));
  const [invalidJsonFields, setInvalidJsonFields] = useState<Set<JsonField>>(() => new Set());

  const api = useCallback(
    async <T,>(path: string, init: RequestInit = {}): Promise<T> => {
      const token = await getToken();
      const headers = new Headers(init.headers);
      if (token === null) headers.set("x-parish-owner-id", "user_synthetic_owner");
      else headers.set("authorization", `Bearer ${token}`);
      if (init.body !== undefined && !(init.body instanceof FormData)) {
        headers.set("content-type", "application/json");
      }
      const response = await fetch(`${apiBase}${path}`, {
        ...init,
        headers,
        credentials: "include",
      });
      if (!response.ok) {
        const body = (await response.json().catch(() => ({}))) as {
          error?: { code?: string; message?: string };
        };
        throw new ApiError(
          body.error?.message ?? `Request failed (${response.status}).`,
          body.error?.code ?? "REQUEST_FAILED",
        );
      }
      return response.status === 204 ? (undefined as T) : ((await response.json()) as T);
    },
    [apiBase, getToken],
  );

  const refresh = useCallback(async () => {
    try {
      const [endpointResponse, keyResponse, invocationResponse, usageResponse, modelResponse] =
        await Promise.all([
          api<{ data: Endpoint[] }>("/api/control/v1/endpoints"),
          api<{ data: ApiKey[] }>("/api/control/v1/api-keys"),
          api<{ data: Invocation[] }>("/api/control/v1/invocations?limit=100"),
          api<{ data: Usage }>("/api/control/v1/usage"),
          api<{ data: ModelOption[] }>("/api/control/v1/models"),
        ]);
      setEndpoints(endpointResponse.data);
      setModels(modelResponse.data);
      setKeys(keyResponse.data);
      setInvocations(invocationResponse.data);
      setUsage(usageResponse.data);
      setSelectedId((current) => current || endpointResponse.data[0]?.id || "");
      setNotice(endpointResponse.data.length === 0 ? "Create your first Endpoint." : "Ready.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Could not load the workspace.");
    }
  }, [api]);

  useEffect(() => {
    const firstModelKey = models[0] === undefined ? "" : modelKey(models[0]);
    setNewModelKey((current) =>
      models.some((model) => modelKey(model) === current) ? current : firstModelKey,
    );
  }, [models]);

  useEffect(() => void refresh(), [refresh]);
  useEffect(() => {
    setInvalidJsonFields(new Set());
    setDraft(null);
    setSavedDefinitionFingerprint("");
    setVersions([]);
    setResult(null);
    if (selectedId === "") return;
    let cancelled = false;
    void Promise.all([
      api<{ data: Draft }>(`/api/control/v1/endpoints/${selectedId}/draft`),
      api<{ data: Version[] }>(`/api/control/v1/endpoints/${selectedId}/versions`),
    ])
      .then(([draftResponse, versionResponse]) => {
        if (cancelled) return;
        setDraft(draftResponse.data);
        setSavedDefinitionFingerprint(
          definitionFingerprint(definitionFromDraft(draftResponse.data)),
        );
        setSchemaText({
          inputSchema: JSON.stringify(draftResponse.data.inputSchema, null, 2),
          outputSchema: JSON.stringify(draftResponse.data.outputSchema, null, 2),
        });
        setVersions(versionResponse.data);
      })
      .catch((error: unknown) =>
        cancelled ? undefined : setNotice(error instanceof Error ? error.message : "Load failed."),
      );
    return () => {
      cancelled = true;
    };
  }, [api, selectedId]);

  function hasValidJsonEditors(): boolean {
    const invalidFields = (Object.keys(schemaText) as JsonField[]).filter(
      (field) => parseJsonObject(schemaText[field]) === null,
    );
    if (invalidFields.length === 0) return true;
    setInvalidJsonFields(new Set(invalidFields));
    setNotice("Fix invalid JSON schemas before saving, testing, or publishing.");
    return false;
  }

  function updateSchema(field: JsonField, text: string): void {
    setSchemaText((current) => ({ ...current, [field]: text }));
    const parsed = parseJsonObject(text);
    setInvalidJsonFields((current) => {
      const next = new Set(current);
      if (parsed === null) next.add(field);
      else next.delete(field);
      return next;
    });
    if (parsed !== null) {
      setDraft((current) => (current === null ? current : { ...current, [field]: parsed }));
    }
  }

  async function createEndpoint(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    const selectedModel = models.find((model) => modelKey(model) === form.get("model"));
    if (selectedModel === undefined) {
      setNotice("Select one of the configured provider models before creating an Endpoint.");
      return;
    }
    try {
      const response = await api<{ data: { endpoint: Endpoint } }>("/api/control/v1/endpoints", {
        method: "POST",
        body: JSON.stringify({
          name: form.get("name"),
          slug: form.get("slug"),
          description: form.get("description"),
          definition: starterDefinition(selectedModel),
        }),
      });
      formElement.reset();
      await refresh();
      setSelectedId(response.data.endpoint.id);
      setNotice("Endpoint created as a mutable draft.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Create failed.");
    }
  }

  function currentDefinition(): Definition | null {
    if (draft === null) return null;
    const inputSchema = parseJsonObject(schemaText.inputSchema);
    const outputSchema = parseJsonObject(schemaText.outputSchema);
    if (inputSchema === null || outputSchema === null) return null;
    return {
      inputSchema,
      outputSchema,
      instructions: draft.instructions,
      providerConfig: draft.providerConfig,
      inferenceConfig: draft.inferenceConfig,
    };
  }

  function canUseSavedDraft(): boolean {
    if (!hasValidJsonEditors()) return false;
    const definition = currentDefinition();
    if (
      draft === null ||
      definition === null ||
      savedDefinitionFingerprint !== definitionFingerprint(definition)
    ) {
      setNotice("Save the draft before testing or publishing.");
      return false;
    }
    return true;
  }

  async function saveDraft() {
    if (!hasValidJsonEditors()) return;
    const definition = currentDefinition();
    if (draft === null || definition === null) return;
    try {
      const response = await api<{ data: Draft }>(
        `/api/control/v1/endpoints/${draft.endpointId}/draft`,
        {
          method: "PUT",
          body: JSON.stringify({ expectedRevision: draft.revision, definition }),
        },
      );
      setDraft(response.data);
      setSavedDefinitionFingerprint(definitionFingerprint(definitionFromDraft(response.data)));
      setSchemaText({
        inputSchema: JSON.stringify(response.data.inputSchema, null, 2),
        outputSchema: JSON.stringify(response.data.outputSchema, null, 2),
      });
      setInvalidJsonFields(new Set());
      setNotice(`Draft saved at revision ${response.data.revision}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Save failed.");
    }
  }

  async function testDraft() {
    if (draft === null || !canUseSavedDraft()) return;
    try {
      const values = JSON.parse(inputText) as Record<string, unknown>;
      let body: BodyInit;
      if (image === null) body = JSON.stringify({ input: values });
      else {
        const form = new FormData();
        form.set("input", JSON.stringify(values));
        form.set("image", image);
        body = form;
      }
      const response = await api<{ data: unknown }>(
        `/api/control/v1/endpoints/${draft.endpointId}/test`,
        { method: "POST", body },
      );
      setResult(response.data);
      await refresh();
      setNotice("Draft test succeeded and passed output validation.");
    } catch (error) {
      setResult(null);
      setNotice(error instanceof Error ? error.message : "Test failed.");
    }
  }

  async function publish() {
    if (draft === null || !canUseSavedDraft()) return;
    try {
      const response = await api<{ data: { version: number } }>(
        `/api/control/v1/endpoints/${draft.endpointId}/publish`,
        { method: "POST", body: JSON.stringify({ expectedRevision: draft.revision }) },
      );
      setNotice(`Published immutable version ${response.data.version}.`);
      await loadVersions(draft.endpointId);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Publish failed.");
    }
  }

  async function loadVersions(endpointId: string) {
    const listed = await api<{ data: Version[] }>(
      `/api/control/v1/endpoints/${endpointId}/versions`,
    );
    setVersions(listed.data);
  }

  async function promote(version: number) {
    if (draft === null) return;
    const revision = versions[0]?.productionAliasRevision ?? null;
    try {
      await api(`/api/control/v1/endpoints/${draft.endpointId}/aliases/production`, {
        method: "PUT",
        body: JSON.stringify({ version, expectedRevision: revision }),
      });
      await loadVersions(draft.endpointId);
      setNotice(`Production now targets version ${version}.`);
    } catch (error) {
      setNotice(
        error instanceof ApiError && error.code === "CONFLICT"
          ? "Alias changed elsewhere; reload before promoting."
          : error instanceof Error
            ? error.message
            : "Promotion failed.",
      );
    }
  }

  async function createKey() {
    const selected = endpoints.find((item) => item.id === selectedId);
    if (selected === undefined) return;
    try {
      const response = await api<{ data: { secret: string; key: ApiKey } }>(
        "/api/control/v1/api-keys",
        {
          method: "POST",
          body: JSON.stringify({
            name: `${selected.name} CLI`,
            scopes: [`invoke:endpoint:${selected.slug}`],
          }),
        },
      );
      setSecret(response.data.secret);
      setKeys((current) => [response.data.key, ...current]);
      setNotice("API key created. Copy it now; it will not be shown again.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Key creation failed.");
    }
  }

  async function revokeKey(keyId: string) {
    await api(`/api/control/v1/api-keys/${keyId}`, { method: "DELETE" });
    setKeys((current) =>
      current.map((item) => (item.id === keyId ? { ...item, status: "revoked" } : item)),
    );
  }

  const renderedDefinition = currentDefinition();
  const draftIsDirty =
    draft !== null &&
    (renderedDefinition === null ||
      savedDefinitionFingerprint === "" ||
      savedDefinitionFingerprint !== definitionFingerprint(renderedDefinition));

  return (
    <main className="workspace">
      <header className="topbar">
        <div>
          <span className="eyebrow">PARISH / ENDPOINTS</span>
          <strong>Owner console</strong>
        </div>
        <span className="notice" role="status">
          {notice}
        </span>
        {onSignOut === undefined ? (
          <span className="devBadge">development</span>
        ) : (
          <div>
            <span className="devBadge">{ownerEmail ?? "owner"}</span>
            <button type="button" onClick={onSignOut}>
              Sign out
            </button>
          </div>
        )}
      </header>
      <aside>
        <form className="newEndpoint" onSubmit={createEndpoint}>
          <h2>New Endpoint</h2>
          <input name="name" aria-label="Endpoint name" placeholder="Image extractor" required />
          <input name="slug" aria-label="Endpoint slug" placeholder="image-extractor" required />
          <input
            name="description"
            aria-label="Description"
            placeholder="What this behavior does"
          />
          <select
            name="model"
            aria-label="Provider model"
            value={newModelKey}
            onChange={(event) => setNewModelKey(event.target.value)}
            disabled={models.length === 0}
            required
          >
            <option value="" disabled>
              {models.length === 0 ? "Loading configured models…" : "Select a provider model"}
            </option>
            {models.map((model) => (
              <option key={modelKey(model)} value={modelKey(model)}>
                {model.provider} / {model.model}
              </option>
            ))}
          </select>
          <button type="submit" disabled={models.length === 0}>
            Create draft
          </button>
        </form>
        <nav aria-label="Endpoints">
          {endpoints.map((endpoint) => (
            <button
              type="button"
              className={endpoint.id === selectedId ? "selected" : ""}
              key={endpoint.id}
              onClick={() => {
                setResult(null);
                setSelectedId(endpoint.id);
              }}
            >
              <strong>{endpoint.name}</strong>
              <code>/{endpoint.slug}</code>
            </button>
          ))}
        </nav>
      </aside>
      <section className="editor">
        {draft === null ? (
          <div className="empty">
            <h1>Define typed AI behavior.</h1>
            <p>Create an Endpoint to begin.</p>
          </div>
        ) : (
          <>
            <div className="sectionTitle">
              <div>
                <span>Mutable draft</span>
                <h1>{endpoints.find((item) => item.id === selectedId)?.name}</h1>
              </div>
              <code>revision {draft.revision}</code>
            </div>
            <label>
              Creator instructions
              <textarea
                value={draft.instructions}
                onChange={(event) =>
                  setDraft((current) =>
                    current === null ? current : { ...current, instructions: event.target.value },
                  )
                }
              />
            </label>
            <div className="split">
              <JsonEditor
                label="Input JSON Schema"
                text={schemaText.inputSchema}
                onChange={(text) => updateSchema("inputSchema", text)}
                invalid={invalidJsonFields.has("inputSchema")}
              />
              <JsonEditor
                label="Output JSON Schema"
                text={schemaText.outputSchema}
                onChange={(text) => updateSchema("outputSchema", text)}
                invalid={invalidJsonFields.has("outputSchema")}
              />
            </div>
            <div className="settings">
              <label>
                Provider model
                <select
                  value={modelKey(draft.providerConfig)}
                  onChange={(event) => {
                    const selected = models.find((model) => modelKey(model) === event.target.value);
                    if (selected !== undefined) {
                      setDraft((current) =>
                        current === null ? current : { ...current, providerConfig: selected },
                      );
                    }
                  }}
                  disabled={models.length === 0}
                >
                  {!models.some((model) => modelKey(model) === modelKey(draft.providerConfig)) && (
                    <option value={modelKey(draft.providerConfig)}>
                      {draft.providerConfig.provider} / {draft.providerConfig.model} (no longer
                      allowed)
                    </option>
                  )}
                  {models.map((model) => (
                    <option key={modelKey(model)} value={modelKey(model)}>
                      {model.provider} / {model.model}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Max tokens
                <input
                  type="number"
                  min="1"
                  max="4096"
                  value={draft.inferenceConfig.maxOutputTokens}
                  onChange={(event) =>
                    setDraft((current) =>
                      current === null
                        ? current
                        : {
                            ...current,
                            inferenceConfig: {
                              ...current.inferenceConfig,
                              maxOutputTokens: Number(event.target.value),
                            },
                          },
                    )
                  }
                />
              </label>
              <label>
                Retries
                <select
                  value={draft.inferenceConfig.retryCount}
                  onChange={(event) =>
                    setDraft((current) =>
                      current === null
                        ? current
                        : {
                            ...current,
                            inferenceConfig: {
                              ...current.inferenceConfig,
                              retryCount: Number(event.target.value) as 0 | 1,
                            },
                          },
                    )
                  }
                >
                  <option value="0">0</option>
                  <option value="1">1</option>
                </select>
              </label>
            </div>
            <div className="actions">
              {draftIsDirty && (
                <span className="notice">Save changes before testing or publishing.</span>
              )}
              <button onClick={saveDraft}>Save draft</button>
              <button className="primary" disabled={draftIsDirty} onClick={publish}>
                Publish immutable version
              </button>
            </div>
            <section className="panel">
              <div className="panelHeading">
                <div>
                  <span>Playground</span>
                  <h2>Test current draft</h2>
                </div>
                <button disabled={draftIsDirty} onClick={testDraft}>
                  Run test
                </button>
              </div>
              <textarea
                className="code short"
                value={inputText}
                onChange={(event) => setInputText(event.target.value)}
              />
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp"
                onChange={(event) => setImage(event.target.files?.[0] ?? null)}
              />
              {result !== null && <pre>{JSON.stringify(result, null, 2)}</pre>}
            </section>
            <section className="panel">
              <div className="panelHeading">
                <div>
                  <span>Deployment alias</span>
                  <h2>Versions</h2>
                </div>
              </div>
              <div className="rows">
                {versions.map((version) => (
                  <div className="row" key={version.id}>
                    <strong>v{version.version}</strong>
                    <code>{version.contentHash.slice(0, 12)}</code>
                    <span>
                      {version.isProduction
                        ? "production"
                        : new Date(version.publishedAt).toLocaleString()}
                    </span>
                    <button
                      disabled={version.isProduction}
                      onClick={() => promote(version.version)}
                    >
                      {version.isProduction ? "Live" : "Promote"}
                    </button>
                  </div>
                ))}
              </div>
            </section>
          </>
        )}
      </section>
      <section className="operations">
        <section className="panel">
          <div className="panelHeading">
            <div>
              <span>Consumer access</span>
              <h2>API keys</h2>
            </div>
            <button disabled={!selectedId} onClick={createKey}>
              Create key
            </button>
          </div>
          {secret && (
            <div className="secret">
              <strong>Copy once</strong>
              <code>{secret}</code>
              <button onClick={() => void navigator.clipboard.writeText(secret)}>Copy</button>
            </div>
          )}
          <div className="rows">
            {keys.map((key) => (
              <div className="row compact" key={key.id}>
                <strong>{key.name}</strong>
                <code>{key.keyPrefix}…</code>
                <span>{key.status}</span>
                <button disabled={key.status === "revoked"} onClick={() => void revokeKey(key.id)}>
                  Revoke
                </button>
              </div>
            ))}
          </div>
        </section>
        <section className="panel">
          <div className="panelHeading">
            <div>
              <span>
                Metadata only · {usage.invocations} calls · {usage.totalTokens} tokens · $
                {usage.estimatedProviderCost}
              </span>
              <h2>Invocation history</h2>
            </div>
            <button onClick={refresh}>Refresh</button>
          </div>
          <div className="rows">
            {invocations.map((invocation) => (
              <div className="invocation" key={invocation.id}>
                <span className={`dot ${invocation.status}`} />
                <div>
                  <strong>
                    {endpoints.find((endpoint) => endpoint.id === invocation.endpointId)?.name ??
                      "Endpoint"}
                    {" · "}
                    {invocationVersionLabel(invocation)}
                  </strong>
                  <span>{invocation.isTest ? "draft test" : "API invocation"}</span>
                  <code>{invocation.requestId}</code>
                  <time dateTime={invocation.startedAt}>
                    {formatStartedAt(invocation.startedAt)}
                  </time>
                </div>
                <span>
                  {invocation.provider}/{invocation.model}
                </span>
                <span>{invocation.durationMs ?? "—"} ms</span>
                <span>{invocation.errorCode ?? invocation.validationStatus}</span>
              </div>
            ))}
          </div>
        </section>
      </section>
    </main>
  );
}

export function CreatorConsole() {
  const getToken = useCallback(async () => null, []);
  return <ConsoleWithToken getToken={getToken} />;
}

export function FirebaseCreatorConsole() {
  const [user, setUser] = useState<User | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");
  const auth = useMemo(() => getFirebaseAuth(), []);
  const getToken = useCallback(() => user?.getIdToken() ?? Promise.resolve(null), [user]);

  useEffect(
    () =>
      onAuthStateChanged(auth, (currentUser) => {
        setUser(currentUser);
        setLoaded(true);
      }),
    [auth],
  );

  if (!loaded) return <main className="signin">Loading owner session…</main>;
  if (user !== null) {
    return (
      <ConsoleWithToken
        getToken={getToken}
        {...(user.email === null ? {} : { ownerEmail: user.email })}
        onSignOut={() => void signOut(auth)}
      />
    );
  }
  return (
    <main className="signin">
      <span className="eyebrow">PARISH / ENDPOINTS</span>
      <h1>Owner access only.</h1>
      <p>Sign in with the configured owner identity to manage Endpoint behavior.</p>
      <button
        className="primary"
        onClick={() => {
          setError("");
          void signInWithPopup(auth, new GoogleAuthProvider()).catch(() =>
            setError("Google sign-in failed. Please try again."),
          );
        }}
      >
        Sign in with Google
      </button>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
