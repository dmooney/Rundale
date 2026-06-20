const STATUS_LABELS = {
    loading: 'Loading scene',
    ready: 'Scene ready',
    empty: 'No scene available',
    sending: 'Sending command',
    error: 'Connection error',
};

export function visualStatusLabel(kind) {
    return STATUS_LABELS[kind] || STATUS_LABELS.loading;
}

export function controlState({ isRefreshing = false, isSending = false } = {}) {
    const busy = Boolean(isRefreshing || isSending);
    return {
        busy,
        disableRefresh: busy,
        disableCommand: busy,
        disableActions: busy,
    };
}
