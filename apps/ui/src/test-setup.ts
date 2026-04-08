import '@testing-library/jest-dom';

// Node 25 ships a global `localStorage` getter that returns an empty stub
// (controlled by `--localstorage-file`) which shadows jsdom's Storage on both
// `globalThis` and `window`. The stub has no methods, so any test that calls
// `localStorage.clear()`/`setItem()`/etc. throws. Install a minimal in-memory
// Storage on both globals for the duration of the test run.
class MemoryStorage {
	private store = new Map<string, string>();
	get length(): number {
		return this.store.size;
	}
	clear(): void {
		this.store.clear();
	}
	getItem(key: string): string | null {
		return this.store.has(key) ? this.store.get(key)! : null;
	}
	setItem(key: string, value: string): void {
		this.store.set(key, String(value));
	}
	removeItem(key: string): void {
		this.store.delete(key);
	}
	key(index: number): string | null {
		return Array.from(this.store.keys())[index] ?? null;
	}
}

function installStorage(name: 'localStorage' | 'sessionStorage') {
	const storage = new MemoryStorage();
	const desc: PropertyDescriptor = {
		value: storage,
		writable: true,
		configurable: true,
		enumerable: true
	};
	Object.defineProperty(globalThis, name, desc);
	if (typeof window !== 'undefined') {
		Object.defineProperty(window, name, desc);
	}
}

installStorage('localStorage');
installStorage('sessionStorage');

// Polyfill window.matchMedia for jsdom (needed by Svelte's tweened/motion)
if (typeof window !== 'undefined' && !window.matchMedia) {
	window.matchMedia = (query: string) =>
		({
			matches: false,
			media: query,
			onchange: null,
			addListener: () => {},
			removeListener: () => {},
			addEventListener: () => {},
			removeEventListener: () => {},
			dispatchEvent: () => false
		}) as MediaQueryList;
}
