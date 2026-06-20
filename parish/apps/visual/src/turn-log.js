export const DEFAULT_TURN_LOG_LIMIT = 8;

export function responseSummary(response) {
    const lines = Array.isArray(response?.lines) ? response.lines : [];
    const last = [...lines].reverse().find((line) => String(line?.text || '').trim());
    return String(last?.text || response?.outcome || 'Done').trim();
}

export function createTurnEntry(kind, label, text) {
    return {
        kind,
        label: String(label || kind || 'Entry'),
        text: String(text || '').trim(),
    };
}

export function appendTurnEntry(entries, entry, limit = DEFAULT_TURN_LOG_LIMIT) {
    const next = [
        ...entries,
        createTurnEntry(entry.kind, entry.label, entry.text),
    ].filter((value) => value.text);
    return next.slice(-Math.max(1, limit));
}
