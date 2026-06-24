import { copyFile, mkdir, readdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const distDir = path.join(appDir, 'dist');
const vendorDir = path.join(distDir, 'vendor');
const srcDir = path.join(distDir, 'src');
const assetsDir = path.join(distDir, 'assets');
const runtimeFiles = [
    'main.js',
    'pixi-renderer.js',
    'renderer.js',
    'scene-client.js',
    'styles.css',
    'turn-log.js',
];

await rm(distDir, { force: true, recursive: true });
await mkdir(distDir, { recursive: true });
await mkdir(vendorDir, { recursive: true });
await mkdir(srcDir, { recursive: true });
await copyDir(path.join(appDir, 'assets'), assetsDir);
await copyFile(path.join(appDir, 'index.html'), path.join(distDir, 'index.html'));
for (const file of runtimeFiles) {
    await copyFile(path.join(appDir, 'src', file), path.join(srcDir, file));
}
await copyFile(
    path.join(appDir, 'node_modules/pixi.js/dist/pixi.mjs'),
    path.join(vendorDir, 'pixi.mjs'),
);

console.log(`Built visual client at ${path.relative(process.cwd(), distDir)}`);

async function copyDir(from, to) {
    await mkdir(to, { recursive: true });
    const entries = await readdir(from, { withFileTypes: true });
    for (const entry of entries) {
        const source = path.join(from, entry.name);
        const target = path.join(to, entry.name);
        if (entry.isDirectory()) {
            await copyDir(source, target);
        } else if (entry.isFile()) {
            await copyFile(source, target);
        }
    }
}
