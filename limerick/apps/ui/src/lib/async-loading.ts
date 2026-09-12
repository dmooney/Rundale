/**
 * Shared loading/error scaffolding for editor panels.
 *
 * Collapses the repeated
 * `loading = true; error = ''; try { … } catch (e) { error = String(e) }
 * finally { loading = false }` block that several editor components carried
 * (TD-039). Svelte 5 `$state` cannot be passed by reference, so callers hand
 * in plain setter callbacks bound to their local runes.
 */
export interface LoadingHandlers {
	/** Toggle the component's `loading` rune. */
	setLoading: (value: boolean) => void;
	/** Write the component's `error` rune (empty string clears it). */
	setError: (message: string) => void;
}

/**
 * Runs `fn` with loading/error bookkeeping. Returns the resolved value on
 * success, or `undefined` if `fn` threw (the error string is reported via
 * `setError`). The error is swallowed by design so callers can branch on the
 * `undefined` result instead of wrapping every call site in its own `try`.
 */
export async function runWithLoading<T>(
	{ setLoading, setError }: LoadingHandlers,
	fn: () => Promise<T>,
): Promise<T | undefined> {
	setLoading(true);
	setError('');
	try {
		return await fn();
	} catch (e) {
		setError(String(e));
		return undefined;
	} finally {
		setLoading(false);
	}
}
