export function apiPathForRequestUrl(requestUrl) {
    const incoming = new URL(requestUrl || '/', 'http://visual.local');
    if (!incoming.pathname.startsWith('/api/')) {
        return null;
    }
    return `${incoming.pathname}${incoming.search}`;
}
