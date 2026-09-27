#!/usr/bin/env node
// Minimal owned server for the already-built UI used by graphical Tauri QA.
// Unlike `vite dev`, it performs no transforms or HMR; it only gives WebKit
// normal HTTP MIME responses for the exact `dist` snapshot the launcher built.
import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, normalize, resolve, sep } from 'node:path';

const args = new Map(
	process.argv
		.slice(2)
		.map((value, index, values) => [value, values[index + 1]]),
);
const root = resolve(args.get('--root') ?? '.');
const port = Number(args.get('--port') ?? 0);
const MIME = {
	'.css': 'text/css; charset=utf-8',
	'.html': 'text/html; charset=utf-8',
	'.ico': 'image/x-icon',
	'.jpeg': 'image/jpeg',
	'.jpg': 'image/jpeg',
	'.js': 'text/javascript; charset=utf-8',
	'.json': 'application/json; charset=utf-8',
	'.png': 'image/png',
	'.svg': 'image/svg+xml',
	'.webp': 'image/webp',
	'.woff2': 'font/woff2',
};

function fileFor(requestUrl) {
	const relative = decodeURIComponent(
		new URL(requestUrl, 'http://localhost').pathname,
	).replace(/^\/+/, '');
	let candidate = resolve(root, normalize(relative));
	if (!candidate.startsWith(`${root}${sep}`) && candidate !== root) return null;
	if (existsSync(candidate) && statSync(candidate).isDirectory())
		candidate = resolve(candidate, 'index.html');
	return candidate;
}

const server = createServer((request, response) => {
	const file = fileFor(request.url ?? '/');
	if (!file || !existsSync(file) || !statSync(file).isFile()) {
		response.writeHead(404).end('not found');
		return;
	}
	response.writeHead(200, {
		'content-type':
			MIME[extname(file).toLowerCase()] ?? 'application/octet-stream',
		'cache-control': 'no-store',
	});
	createReadStream(file).pipe(response);
});
server.listen(port, '127.0.0.1', () => {
	const address = server.address();
	if (!address || typeof address === 'string')
		throw new Error('no loopback address');
	console.log(`READY http://127.0.0.1:${address.port}`);
});
for (const signal of ['SIGINT', 'SIGTERM'])
	process.on(signal, () => server.close(() => process.exit(0)));
