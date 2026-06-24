export const proxySessionCookieName = 'parish_visual_sid';

export function parseCookieHeader(header) {
    return Object.fromEntries(
        String(header || '')
            .split(';')
            .map((part) => part.trim())
            .filter(Boolean)
            .map((part) => {
                const index = part.indexOf('=');
                if (index === -1) {
                    return [part, ''];
                }
                return [part.slice(0, index), part.slice(index + 1)];
            }),
    );
}

export function createProxySessionStore({ cookieName = proxySessionCookieName } = {}) {
    let nextSessionId = 1;
    const backendCookiesBySession = new Map();

    function sessionForRequest(cookieHeader) {
        const cookies = parseCookieHeader(cookieHeader);
        const existing = cookies[cookieName];
        if (existing) {
            return { id: existing, created: false };
        }
        const id = `v${nextSessionId++}`;
        return { id, created: true };
    }

    function backendCookieFor(sessionId) {
        return backendCookiesBySession.get(sessionId) || '';
    }

    function rememberBackendCookie(sessionId, setCookie) {
        const values = Array.isArray(setCookie) ? setCookie : setCookie ? [setCookie] : [];
        const cookies = values
            .map((value) => String(value).split(';', 1)[0])
            .filter(Boolean);
        if (cookies.length > 0) {
            backendCookiesBySession.set(sessionId, cookies.join('; '));
        }
    }

    function clientSetCookie(sessionId) {
        return `${cookieName}=${sessionId}; Path=/; SameSite=Lax; HttpOnly`;
    }

    return {
        backendCookieFor,
        clientSetCookie,
        rememberBackendCookie,
        sessionForRequest,
    };
}
