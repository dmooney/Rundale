import { readdir } from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ignoredDirs = new Set(['dist', 'node_modules']);

async function collectJavaScriptFiles(dir) {
    const entries = await readdir(dir, { withFileTypes: true });
    const files = [];
    for (const entry of entries) {
        if (entry.isDirectory()) {
            if (!ignoredDirs.has(entry.name)) {
                files.push(...(await collectJavaScriptFiles(path.join(dir, entry.name))));
            }
            continue;
        }
        if (entry.isFile() && /\.(mjs|js)$/.test(entry.name)) {
            files.push(path.join(dir, entry.name));
        }
    }
    return files;
}

const files = await collectJavaScriptFiles(appDir);
let failed = false;

for (const file of files) {
    const result = spawnSync(process.execPath, ['--check', file], {
        encoding: 'utf8',
        stdio: 'pipe',
    });
    if (result.status !== 0) {
        failed = true;
        process.stderr.write(result.stderr);
    }
}

if (failed) {
    process.exit(1);
}

console.log(`Syntax check passed for ${files.length} file(s).`);
