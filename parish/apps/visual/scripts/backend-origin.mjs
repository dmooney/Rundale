const allowedBackendOrigins = new Map([
    ['3030', 'http://127.0.0.1:3030'],
    ['3001', 'http://127.0.0.1:3001'],
]);

function isLoopbackHost(hostname) {
    const normalized = String(hostname || '').toLowerCase();
    return normalized === 'localhost' || normalized === '127.0.0.1' || normalized === '[::1]';
}

function originForPort(port) {
    const origin = allowedBackendOrigins.get(String(port || '').trim());
    if (!origin) {
        throw new Error('Visual dev proxy only allows local Parish backend ports 3030 or 3001');
    }
    return origin;
}

export function backendOriginFromEnv(env = process.env) {
    const configuredUrl = String(env.PARISH_BACKEND_URL || '').trim();
    if (configuredUrl) {
        const url = new URL(configuredUrl);
        if (url.protocol !== 'http:' || !isLoopbackHost(url.hostname)) {
            throw new Error('PARISH_BACKEND_URL must be an http:// loopback URL');
        }
        return originForPort(url.port || '80');
    }
    return originForPort(env.PARISH_BACKEND_PORT || '3030');
}

export function proxyTargetUrl(requestUrl, backendOrigin) {
    const incoming = new URL(requestUrl || '/', 'http://visual.local');
    return new URL(`${incoming.pathname}${incoming.search}`, backendOrigin);
}
