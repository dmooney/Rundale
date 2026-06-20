import { createServer } from 'node:http';
import { stat } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import { Readable } from 'node:stream';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const port = Number.parseInt(process.env.VISUAL_CLIENT_PORT || process.env.PORT || '4174', 10);
const backendUrl = process.env.PARISH_BACKEND_URL || 'http://127.0.0.1:3030';

const contentTypes = new Map([
    ['.css', 'text/css; charset=utf-8'],
    ['.html', 'text/html; charset=utf-8'],
    ['.js', 'text/javascript; charset=utf-8'],
    ['.json', 'application/json; charset=utf-8'],
    ['.png', 'image/png'],
    ['.svg', 'image/svg+xml'],
]);

function sendText(res, status, text) {
    res.writeHead(status, { 'content-type': 'text/plain; charset=utf-8' });
    res.end(text);
}

function staticPathFor(url) {
    const pathname = decodeURIComponent(url.pathname);
    const rel = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '');
    const filePath = path.resolve(appDir, rel);
    if (!filePath.startsWith(`${appDir}${path.sep}`) && filePath !== appDir) {
        return null;
    }
    return filePath;
}

async function serveStatic(req, res, url) {
    const filePath = staticPathFor(url);
    if (!filePath) {
        sendText(res, 403, 'Forbidden');
        return;
    }

    let info;
    try {
        info = await stat(filePath);
    } catch (_error) {
        sendText(res, 404, 'Not found');
        return;
    }

    if (!info.isFile()) {
        sendText(res, 404, 'Not found');
        return;
    }

    const contentType = contentTypes.get(path.extname(filePath)) || 'application/octet-stream';
    res.writeHead(200, { 'content-type': contentType });
    createReadStream(filePath).pipe(res);
}

async function proxyApi(req, res) {
    const target = new URL(req.url || '/', backendUrl);
    const headers = new Headers(req.headers);
    headers.delete('host');

    try {
        const upstream = await fetch(target, {
            method: req.method,
            headers,
            body: req.method === 'GET' || req.method === 'HEAD' ? undefined : req,
            duplex: 'half',
        });
        const responseHeaders = Object.fromEntries(upstream.headers.entries());
        res.writeHead(upstream.status, responseHeaders);
        if (!upstream.body) {
            res.end();
            return;
        }
        Readable.fromWeb(upstream.body).pipe(res);
    } catch (error) {
        sendText(res, 502, `Backend unavailable: ${error instanceof Error ? error.message : error}`);
    }
}

const server = createServer(async (req, res) => {
    const url = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
    if (url.pathname.startsWith('/api/')) {
        await proxyApi(req, res);
        return;
    }
    await serveStatic(req, res, url);
});

server.listen(port, '127.0.0.1', () => {
    console.log(`Parish Visual: http://127.0.0.1:${port}`);
    console.log(`Proxying /api/* to ${backendUrl}`);
});
