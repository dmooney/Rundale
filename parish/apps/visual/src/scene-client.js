const SCENE_ENDPOINT = '/api/scene-state';
const COMMAND_ENDPOINT = '/api/command';

export function normalizeBackendUrl(value) {
    return String(value || '').trim().replace(/\/+$/, '');
}

export function resolveApiUrl(backendUrl, endpoint = SCENE_ENDPOINT) {
    const path = endpoint.startsWith('/') ? endpoint : `/${endpoint}`;
    const normalized = normalizeBackendUrl(backendUrl);
    if (!normalized) {
        return path;
    }
    return new URL(path, `${normalized}/`).toString();
}

export async function fetchSceneState({ backendUrl = '', fetchImpl = globalThis.fetch } = {}) {
    if (typeof fetchImpl !== 'function') {
        throw new TypeError('fetchSceneState requires a fetch implementation');
    }

    const response = await fetchImpl(resolveApiUrl(backendUrl), {
        headers: { accept: 'application/json' },
    });
    if (!response.ok) {
        throw new Error(`Scene-state request failed with HTTP ${response.status}`);
    }
    return (await response.json()) ?? null;
}

export async function postCommand({
    text,
    backendUrl = '',
    fetchImpl = globalThis.fetch,
    timeoutMs = 60000,
} = {}) {
    const trimmed = String(text || '').trim();
    if (!trimmed) {
        throw new TypeError('postCommand requires command text');
    }
    const response = await fetchImpl(resolveApiUrl(backendUrl, COMMAND_ENDPOINT), {
        method: 'POST',
        headers: {
            accept: 'application/json',
            'content-type': 'application/json',
        },
        body: JSON.stringify({
            text: trimmed,
            includeState: true,
            includeMap: false,
            timeoutMs,
        }),
    });
    if (!response.ok) {
        throw new Error(`Command request failed with HTTP ${response.status}`);
    }
    return response.json();
}
