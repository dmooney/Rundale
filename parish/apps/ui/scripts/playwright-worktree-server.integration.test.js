import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import {
	PLAYWRIGHT_BUILD_ID_HEADER,
	allocateLoopbackPort,
	binaryContainsExpectedBuildIdentity,
	binaryContainsExpectedCsp,
	cargoBuildArgs,
	prepareIsolatedServerBinary,
	runCargoBuild,
	waitForServedCsp,
} from './playwright-worktree-server.js';

const UI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PARISH_DIR = resolve(UI_DIR, '../..');

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
		}
	},
);
