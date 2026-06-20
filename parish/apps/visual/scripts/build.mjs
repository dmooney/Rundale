import { cp, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const distDir = path.join(appDir, 'dist');

await rm(distDir, { force: true, recursive: true });
await mkdir(distDir, { recursive: true });
await cp(path.join(appDir, 'index.html'), path.join(distDir, 'index.html'));
await cp(path.join(appDir, 'src'), path.join(distDir, 'src'), { recursive: true });

console.log(`Built visual client at ${path.relative(process.cwd(), distDir)}`);
