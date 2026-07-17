import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	statSync,
	utimesSync,
	writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import {
	PLAYWRIGHT_BUILD_ID_HEADER,
	allocateLoopbackPort,
	binaryContainsExpectedBuildIdentity,
	binaryContainsExpectedCsp,
	captureUiDist,
	cargoBuildArgs,
	prepareIsolatedServerBinary,
	pruneServerArtifacts,
	publishActiveUseLease,
	publishCachedBinary,
	publishUiSnapshot,
	runCargoBuild,
	waitForServedCsp,
} from './playwright-worktree-server.js';

const UI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PARISH_DIR = resolve(UI_DIR, '../..');

function backdate(path, milliseconds) {
	const old = new Date(Date.now() - milliseconds);
	utimesSync(path, old, old);
}

test(
	'ordinary Cargo overwrite with identical UI hashes is rejected before live readiness',
	{ timeout: 300_000 },
	async () => {
		let competed = false;
		const prepared = await prepareIsolatedServerBinary({
			async afterCargoBuild({ attempt }) {
				if (attempt !== 1) return;
				competed = true;
				const ordinaryEnv = { ...process.env };
				delete ordinaryEnv.PARISH_PLAYWRIGHT_BUILD_ID;
				delete ordinaryEnv.PARISH_UI_DIST_DIGEST;
				delete ordinaryEnv.PARISH_UI_DIST_DIR;
				const ordinary = await runCargoBuild(cargoBuildArgs(), {
					cwd: PARISH_DIR,
					env: ordinaryEnv,
				});
				assert.equal(ordinary.status, 0);
			},
		});

		assert.equal(competed, true);
		assert.ok(prepared.attempts >= 2);
		const binary = readFileSync(prepared.path);
		assert.equal(
			binaryContainsExpectedBuildIdentity(binary, prepared.buildId),
			true,
		);
		assert.equal(
			binaryContainsExpectedCsp(binary, prepared.expectedHashes),
			true,
		);

		const port = await allocateLoopbackPort();
		const runId = 'integrationrace0123456789';
		const readyFile = join(prepared.outputDir, `.playwright-ready-${runId}`);
		rmSync(readyFile, { force: true });
		const server = spawn(
			prepared.path,
			['--port', String(port), '--static-dir', prepared.staticDir],
			{
				cwd: PARISH_DIR,
				env: {
					...process.env,
					PARISH_PLAYWRIGHT_BUILD_ID: prepared.buildId,
					PARISH_PLAYWRIGHT_READY_FILE: readyFile,
					PARISH_PLAYWRIGHT_RUN_ID: runId,
				},
				stdio: 'ignore',
			},
		);
		try {
			await waitForServedCsp({
				buildId: prepared.buildId,
				expectedHashes: prepared.expectedHashes,
				port,
				readyFile,
				runId,
				server,
			});
			const response = await fetch(
				`http://127.0.0.1:${port}/api/playwright-ready/${runId}`,
			);
			assert.equal(response.status, 200);
			assert.equal(
				response.headers.get(PLAYWRIGHT_BUILD_ID_HEADER),
				prepared.buildId,
			);
		} finally {
			server.kill('SIGTERM');
			if (server.exitCode === null) await once(server, 'exit');
			prepared.activeUseLease.release();
		}
	},
);

test(
	'fresh lease keeps an oldest live static tree readable until lease expiry',
	{ timeout: 300_000 },
	async () => {
		const prepared = await prepareIsolatedServerBinary();
		const root = mkdtempSync(join(tmpdir(), 'parish-playwright-live-lease-'));
		let activeUseLease;
		let server;
		try {
			const capture = captureUiDist(prepared.staticDir);
			assert.ok(capture);
			const staticDir = publishUiSnapshot(root, capture);
			const binaryPath = publishCachedBinary(
				root,
				readFileSync(prepared.path),
				statSync(prepared.path).mode,
			);
			activeUseLease = publishActiveUseLease(root, [binaryPath, staticDir]);
			const leasePath = activeUseLease.path;
			backdate(binaryPath, 120_000);
			backdate(staticDir, 120_000);

			for (let index = 0; index < 3; index += 1) {
				const binary = join(root, `parish-server-fixture-${index}`);
				const snapshot = join(root, `ui-dist-fixture-${index}`);
				writeFileSync(binary, String(index));
				mkdirSync(snapshot);
				writeFileSync(join(snapshot, 'index.html'), String(index));
				backdate(binary, 60_000 - index * 10_000);
				backdate(snapshot, 60_000 - index * 10_000);
			}

			const port = await allocateLoopbackPort();
			const runId = 'integrationactivelease012345';
			const readyFile = join(root, `.playwright-ready-${runId}`);
			server = spawn(
				binaryPath,
				['--port', String(port), '--static-dir', staticDir],
				{
					cwd: PARISH_DIR,
					env: {
						...process.env,
						PARISH_PLAYWRIGHT_BUILD_ID: prepared.buildId,
						PARISH_PLAYWRIGHT_READY_FILE: readyFile,
						PARISH_PLAYWRIGHT_RUN_ID: runId,
					},
					stdio: 'ignore',
				},
			);
			await waitForServedCsp({
				buildId: prepared.buildId,
				expectedHashes: prepared.expectedHashes,
				port,
				readyFile,
				runId,
				server,
			});

			const first = await fetch(`http://127.0.0.1:${port}/`);
			assert.equal(first.status, 200);
			const firstHtml = await first.text();
			pruneServerArtifacts(root, {
				maxArtifacts: 3,
				staleGraceMs: 0,
			});
			assert.equal(existsSync(binaryPath), true);
			assert.equal(existsSync(staticDir), true);
			const second = await fetch(`http://127.0.0.1:${port}/`);
			assert.equal(second.status, 200);
			assert.equal(await second.text(), firstHtml);

			server.kill('SIGTERM');
			if (server.exitCode === null) await once(server, 'exit');
			activeUseLease.release();
			activeUseLease = undefined;
			assert.equal(existsSync(leasePath), false);

			const newestBinary = join(root, 'parish-server-fixture-newest');
			const newestSnapshot = join(root, 'ui-dist-fixture-newest');
			writeFileSync(newestBinary, 'newest');
			mkdirSync(newestSnapshot);
			writeFileSync(join(newestSnapshot, 'index.html'), 'newest');
			pruneServerArtifacts(root, {
				maxArtifacts: 3,
				staleGraceMs: 0,
			});
			assert.equal(existsSync(binaryPath), false);
			assert.equal(existsSync(staticDir), false);
		} finally {
			if (server?.exitCode === null && server?.signalCode === null) {
				server.kill('SIGKILL');
				await once(server, 'exit');
			}
			activeUseLease?.release();
			prepared.activeUseLease.release();
			rmSync(root, { force: true, recursive: true });
		}
	},
);
