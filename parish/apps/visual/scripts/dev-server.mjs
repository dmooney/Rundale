import { createServer, request as httpRequest } from 'node:http';
import { stat } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { apiPathForRequestUrl } from './api-path.mjs';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const port = Number.parseInt(process.env.VISUAL_CLIENT_PORT || process.env.PORT || '4174', 10);
const backendHost = '127.0.0.1';
const backendPort = 3030;

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
    const apiPath = apiPathForRequestUrl(req.url);
    if (!apiPath) {
        sendText(res, 404, 'Not found');
        return;
    }
    const headers = { ...req.headers };
    delete headers.host;

    const upstream = httpRequest(
        {
            hostname: backendHost,
            port: backendPort,
            path: apiPath,
            method: req.method,
            headers,
        },
        (upstreamResponse) => {
            res.writeHead(upstreamResponse.statusCode || 502, upstreamResponse.headers);
            upstreamResponse.pipe(res);
        },
    );
    upstream.on('error', (error) => {
        sendText(res, 502, `Backend unavailable: ${error instanceof Error ? error.message : error}`);
    });
    if (req.method === 'GET' || req.method === 'HEAD') {
        upstream.end();
    } else {
        req.pipe(upstream);
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
    console.log(`Proxying /api/* to http://${backendHost}:${backendPort}`);
});
