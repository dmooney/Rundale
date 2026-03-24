import { vi } from 'vitest';

export const listen = vi.fn(async (_event: string, _handler: unknown) => {
	// Returns an unlisten function
	return () => {};
});
