import { setWorkerUrl } from 'maplibre-gl';
import mapLibreWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';

let configured = false;

/**
 * Points MapLibre at the worker asset emitted by Vite.
 *
 * MapLibre 6 resolves its worker next to `import.meta.url`. Once a bundler
 * folds the library into a hashed application chunk, that default points at
 * a file Vite never emitted. Importing the module with `?worker&url` makes
 * Vite bundle its shared-module dependency into a complete worker asset and
 * supplies that asset's final hashed URL here.
 */
export function configureMapLibreWorker(): void {
	if (configured) return;
	setWorkerUrl(mapLibreWorkerUrl);
	configured = true;
}
