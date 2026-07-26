// @vitest-environment node

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import {
	mkdir,
	mkdtemp,
	readFile,
	readdir,
	rm,
	writeFile,
} from 'node:fs/promises';
import { hostname, tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { PNG } from 'pngjs';
import { afterEach, test } from 'vitest';

import {
	reprocessCandidateFailure,
	reprocessCandidateManifest,
	reprocessCandidateReceipt,
	runCandidateGeneration,
} from './generate-notebook-person-candidates.mjs';

const cleanupPaths = [];
const cleanupServers = [];

function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
	return `{${Object.entries(value)
		.filter(([, child]) => child !== undefined)
		.sort(([left], [right]) => left.localeCompare(right))
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

function canonicalSha256(value) {
	return createHash('sha256').update(canonicalJson(value)).digest('hex');
}

afterEach(async () => {
	await Promise.all(
		cleanupServers.splice(0).map(
			(server) =>
				new Promise((resolveClose, rejectClose) => {
					server.close((error) =>
						error ? rejectClose(error) : resolveClose(),
					);
				}),
		),
	);
	await Promise.all(
		cleanupPaths
			.splice(0)
			.map((path) => rm(path, { recursive: true, force: true })),
	);
});

function imageFixture({
	blank = false,
	coloredFill = false,
	coloredFringe = false,
	dense = false,
	magentaFringe = false,
	nearKeyCorner = false,
	nearKeyBottomBorder = false,
	lowMargin = false,
	oversized = false,
	touchBottom = false,
} = {}) {
	const png = new PNG({ width: 64, height: 64 });
	for (let y = 0; y < png.height; y += 1) {
		for (let x = 0; x < png.width; x += 1) {
			const offset = (y * png.width + x) * 4;
			png.data[offset] = 255;
			png.data[offset + 1] = 0;
			png.data[offset + 2] = 255;
			png.data[offset + 3] = 255;
			const top = lowMargin ? 3 : oversized ? 8 : 17;
			const bottom = touchBottom ? 63 : lowMargin ? 60 : oversized ? 55 : 46;
			const left = lowMargin ? 10 : oversized ? 14 : 20;
			const right = lowMargin ? 53 : oversized ? 49 : 43;
			const inBounds = x >= left && x <= right && y >= top && y <= bottom;
			const sparseInk =
				inBounds &&
				(x === left ||
					x === right ||
					y === top ||
					y === bottom ||
					x === 30 ||
					(y === 31 && x >= 24 && x <= 39));
			const denseInk = x >= 10 && x <= 53 && y >= 8 && y <= 55;
			if (!blank && (dense ? denseInk : sparseInk)) {
				const value = 35 + ((x + y) % 8) * 18;
				png.data[offset] = value;
				png.data[offset + 1] = value;
				png.data[offset + 2] = value;
			}
			if (!blank && coloredFill && x >= 26 && x <= 37 && y >= 24 && y <= 36) {
				png.data[offset] = 0;
				png.data[offset + 1] = 85;
				png.data[offset + 2] = 255;
			}
			if (!blank && coloredFringe && inBounds && [22, 26, 34, 38].includes(x)) {
				png.data[offset] = 110;
				png.data[offset + 1] = 40;
				png.data[offset + 2] = 10;
			}
			if (
				!blank &&
				magentaFringe &&
				(x === 19 || x === 44) &&
				y >= 17 &&
				y <= bottom
			) {
				png.data[offset] = 96;
				png.data[offset + 1] = 24;
				png.data[offset + 2] = 96;
			}
		}
	}
	if (nearKeyCorner) {
		const offset = (png.height - 1) * png.width * 4;
		png.data[offset] = 235;
		png.data[offset + 1] = 31;
		png.data[offset + 2] = 235;
	}
	if (nearKeyBottomBorder) {
		for (let x = 0; x < png.width; x += 1) {
			const offset = ((png.height - 1) * png.width + x) * 4;
			png.data[offset] = 244;
			png.data[offset + 1] = 19;
			png.data[offset + 2] = 218;
		}
	}
	return PNG.sync.write(png, { colorType: 6 });
}

function pairedImageFixture({ portrait = {}, marker = {} } = {}) {
	const left = PNG.sync.read(imageFixture(portrait));
	const right = PNG.sync.read(imageFixture(marker));
	const sheet = new PNG({
		width: left.width + right.width,
		height: left.height,
	});
	for (let row = 0; row < sheet.height; row += 1) {
		const rowBytes = left.width * 4;
		left.data.copy(
			sheet.data,
			row * sheet.width * 4,
			row * rowBytes,
			(row + 1) * rowBytes,
		);
		right.data.copy(
			sheet.data,
			(row * sheet.width + left.width) * 4,
			row * rowBytes,
			(row + 1) * rowBytes,
		);
	}
	return PNG.sync.write(sheet, { colorType: 6 });
}

async function fixtureFiles({ paired = false } = {}) {
	const root = await mkdtemp(join(tmpdir(), 'rundale-art-candidates-'));
	cleanupPaths.push(root);
	const referencePath = join(root, 'portrait-style.png');
	const markerReferencePath = join(root, 'notebook-concept.png');
	const configPath = join(root, 'config.json');
	const inputsPath = join(root, 'inputs.json');
	const outputRoot = join(root, 'candidates');
	const markerIdentity = {
		composition: 'character-only',
		silhouette: 'compact villager in a plain coat',
		stance: 'balanced upright stance',
		empty_hand_pose: 'both-at-sides',
		readability_cues: [
			{ kind: 'body-shape', description: 'compact build' },
			{ kind: 'clothing', description: 'squared coat hem' },
		],
		tiny_readability_notes: ['readable from the person alone'],
	};
	await writeFile(referencePath, imageFixture());
	await writeFile(markerReferencePath, imageFixture());
	await writeFile(
		configPath,
		JSON.stringify({
			schema_version: 1,
			pipeline_revision: 'test-v1',
			provider: {
				id: 'openai',
				adapter: 'openai-images-edits-v1',
				base_url: 'https://api.openai.invalid/v1',
				base_url_env: 'RUNDALE_ART_OPENAI_BASE_URL',
				api_key_env: 'OPENAI_API_KEY',
				model: 'gpt-image-2-test-snapshot',
				endpoint: '/images/edits',
				request: {
					size: paired ? '128x64' : '64x64',
					quality: 'high',
					output_format: 'png',
					background: 'opaque',
					moderation: 'auto',
				},
			},
			reference_inputs: [
				{
					id: 'portrait-style',
					path: referencePath,
					purpose: 'sparse portrait line style only',
					asset_kinds: [paired ? 'pair' : 'portrait'],
				},
				{
					id: 'notebook-concept',
					path: markerReferencePath,
					purpose: 'painted world marker style only',
					asset_kinds: [paired ? 'pair' : 'marker'],
				},
			],
			candidate_count: 1,
			include_fallback: true,
			raw_output: {
				key_color: '#ff00ff',
				postprocess_revision: 'test-key-v2',
				portrait_ink_color: '#36362e',
				framing_normalization: {
					enabled: true,
					algorithm: 'premultiplied-bilinear-v1',
					headroom_fraction: 0.02,
					min_axis_pixels: 2,
				},
				pair_contract: 'left portrait and right marker, same person',
				portrait_contract: 'flat key portrait',
				marker_contract:
					'flat key character-only marker, empty hands, no scenery',
			},
			validation: {
				key_distance: 40,
				key_feather_distance: 136,
				key_spill_chroma_min: 24,
				key_spill_balance_max: 40,
				min_key_fraction: 0.45,
				max_key_fraction: 0.985,
				min_subject_fraction: 0.005,
				max_subject_fraction: 0.55,
				min_subject_color_buckets: 2,
				require_keyed_corners: true,
				ink_luminance_max: 192,
				ink_chroma_max: 72,
				asset_contracts: {
					portrait: {
						max_subject_fraction: 0.2,
						min_ink_bounds_height_fraction: 0.35,
						max_ink_bounds_height_fraction: 0.7,
						max_ink_fraction: 0.09,
						max_ink_fill_fraction: 0.3,
						max_light_subject_fraction: 0.08,
						max_colored_subject_fraction: 0.04,
						max_colored_fill_fraction: 0.003,
					},
					marker: {
						max_subject_fraction: 0.2,
						min_subject_bounds_height_fraction: 0.35,
						max_subject_bounds_height_fraction: 0.8,
						max_subject_bounds_width_fraction: 0.5,
						min_subject_margin_fraction: 0.05,
						max_residual_key_spill_fraction: 0.002,
					},
				},
			},
			rate_limit: {
				requests_per_minute: 60000,
				max_concurrency: 2,
			},
			retry: {
				max_attempts: 1,
				initial_delay_ms: 1,
				max_delay_ms: 2,
				request_timeout_ms: 5000,
			},
			storage: { root: outputRoot, layout: 'content-addressed-v1' },
			approval: {
				generated_status: 'candidate',
				review_status: 'pending',
				auto_promote: false,
			},
			...(paired
				? {
						generation_mode: 'paired-v1',
						paired_output: {
							cell_size: '64x64',
							portrait: { x: 0, y: 0 },
							marker: { x: 64, y: 0 },
						},
					}
				: {}),
		}),
	);
	await writeFile(
		inputsPath,
		JSON.stringify({
			schema_version: 3,
			fallback: {
				pair_prompt: 'Unknown pair prompt',
				portrait_prompt: 'Unknown portrait prompt',
				marker_prompt: 'Unknown marker prompt',
				art_direction: {
					fallback: true,
					marker_identity: markerIdentity,
				},
			},
			npcs: [
				{
					npc_id: 1,
					name: 'Bridget Test',
					pair_prompt: 'Bridget pair prompt',
					portrait_prompt: 'Bridget portrait prompt',
					marker_prompt: 'Bridget marker prompt',
					art_direction: {
						identity: 'bridget',
						marker_identity: markerIdentity,
					},
				},
				{
					npc_id: 2,
					name: 'Cormac Test',
					pair_prompt: 'Cormac pair prompt',
					portrait_prompt: 'Cormac portrait prompt',
					marker_prompt: 'Cormac marker prompt',
					art_direction: {
						identity: 'cormac',
						marker_identity: markerIdentity,
					},
				},
			],
		}),
	);
	return {
		root,
		referencePath,
		markerReferencePath,
		configPath,
		inputsPath,
		outputRoot,
	};
}

async function mockProvider(buffer) {
	const requests = [];
	const server = createServer(async (request, response) => {
		const chunks = [];
		for await (const chunk of request) chunks.push(chunk);
		requests.push({
			url: request.url,
			headers: request.headers,
			body: Buffer.concat(chunks),
		});
		response.writeHead(200, {
			'content-type': 'application/json',
			'x-request-id': 'req_test_notebook_art',
		});
		response.end(
			JSON.stringify({
				created: 1_783_800_000,
				data: [{ b64_json: buffer.toString('base64') }],
				usage: { input_tokens: 12, output_tokens: 34 },
			}),
		);
	});
	await new Promise((resolveListen) =>
		server.listen(0, '127.0.0.1', resolveListen),
	);
	cleanupServers.push(server);
	const address = server.address();
	return { server, requests, baseUrl: `http://127.0.0.1:${address.port}/v1` };
}

async function mockProviderFailure() {
	const requests = [];
	const server = createServer(async (request, response) => {
		const chunks = [];
		for await (const chunk of request) chunks.push(chunk);
		requests.push({
			url: request.url,
			headers: request.headers,
			body: Buffer.concat(chunks),
		});
		response.writeHead(400, {
			'content-type': 'application/json',
			'x-request-id': 'req_test_billing_limit',
		});
		response.end(
			JSON.stringify({
				error: {
					message: 'Billing hard limit has been reached.',
					type: 'billing_limit_user_error',
					code: 'billing_hard_limit_reached',
				},
			}),
		);
	});
	await new Promise((resolveListen) =>
		server.listen(0, '127.0.0.1', resolveListen),
	);
	cleanupServers.push(server);
	const address = server.address();
	return { server, requests, baseUrl: `http://127.0.0.1:${address.port}/v1` };
}

async function mockProviderSequence(steps) {
	const requests = [];
	const server = createServer(async (request, response) => {
		const chunks = [];
		for await (const chunk of request) chunks.push(chunk);
		const index = requests.length;
		requests.push({
			url: request.url,
			headers: request.headers,
			body: Buffer.concat(chunks),
		});
		const step = steps[Math.min(index, steps.length - 1)];
		if (step.delayMs) {
			await new Promise((resolveDelay) =>
				setTimeout(resolveDelay, step.delayMs),
			);
		}
		if (response.destroyed) return;
		response.writeHead(step.status ?? 200, {
			'content-type': 'application/json',
			'x-request-id': step.requestId ?? `req_sequence_${index + 1}`,
		});
		response.end(
			JSON.stringify(
				step.body ?? {
					created: 1_783_800_000 + index,
					data: [{ b64_json: step.buffer.toString('base64') }],
					usage: { input_tokens: 10 + index, output_tokens: 20 + index },
				},
			),
		);
	});
	await new Promise((resolveListen) =>
		server.listen(0, '127.0.0.1', resolveListen),
	);
	cleanupServers.push(server);
	const address = server.address();
	return { server, requests, baseUrl: `http://127.0.0.1:${address.port}/v1` };
}

test('plan mode creates stable, selectable jobs without provider access', async () => {
	const fixture = await fixtureFiles();
	const options = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'all',
		candidateCount: 2,
	};
	const first = await runCandidateGeneration(options);
	const second = await runCandidateGeneration(options);
	assert.equal(first.plan.mode, 'plan');
	assert.equal(first.plan.requests.pending, 4);
	assert.deepEqual(
		first.plan.jobs.map((job) => job.job_id),
		second.plan.jobs.map((job) => job.job_id),
	);
	assert(first.plan.jobs.every((job) => job.name === 'Bridget Test'));
	assert.equal(first.plan.approval.auto_promote, false);
});

test('input validation rejects a prop-driven marker contract', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const inputs = JSON.parse(await readFile(fixture.inputsPath, 'utf8'));
	inputs.npcs[0].art_direction.marker_identity.readable_props = ['hammer'];
	await writeFile(fixture.inputsPath, JSON.stringify(inputs));

	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'pair',
		}),
		/NPC 1 marker identity must not define readable_props/,
	);
});

test('provider refusal keeps request provenance and remains exactly retryable', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const failedProvider = await mockProviderFailure();
	const common = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
	};
	await assert.rejects(
		runCandidateGeneration({
			...common,
			execute: true,
			maxRequests: 1,
			runId: 'provider-refusal',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: failedProvider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	assert.equal(failedProvider.requests.length, 1);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const relativeFailurePath = files.find((path) =>
		path.endsWith('failure.json'),
	);
	assert(relativeFailurePath);
	const failurePath = join(fixture.outputRoot, relativeFailurePath);
	const failureBytes = await readFile(failurePath);
	const failure = JSON.parse(failureBytes.toString('utf8'));
	assert.equal(failure.artifact, null);
	assert.equal(failure.provider.request_id, 'req_test_billing_limit');
	assert.equal(failure.provider.attempts, 1);
	assert.equal(failure.error.status, 400);
	assert.equal(failure.error.request_id, 'req_test_billing_limit');
	assert.equal(failure.error.provider_code, 'billing_hard_limit_reached');
	assert.equal(failure.error.provider_type, 'billing_limit_user_error');

	const retryPlan = await runCandidateGeneration(common);
	assert.equal(retryPlan.plan.requests.resumable_existing, 0);
	assert.equal(retryPlan.plan.requests.pending, 1);

	const successfulProvider = await mockProvider(pairedImageFixture());
	const retried = await runCandidateGeneration({
		...common,
		execute: true,
		maxRequests: 1,
		runId: 'provider-refusal-retry',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: successfulProvider.baseUrl,
		},
	});
	assert.equal(retried.plan.result_counts.generated, 1);
	assert.deepEqual(await readFile(failurePath), failureBytes);
});

test('batch-fatal provider refusal opens a circuit and leaves untouched jobs pending', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProviderFailure();
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1', '2'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 2,
			concurrency: 1,
			runId: 'provider-circuit',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed and 1 were not attempted/,
	);
	assert.equal(provider.requests.length, 1);
	const runRoot = join(fixture.outputRoot, 'runs', 'provider-circuit');
	const summary = JSON.parse(await readFile(join(runRoot, 'run.json'), 'utf8'));
	assert.deepEqual(summary.result_counts, {
		generated: 0,
		resume_skip: 0,
		failed: 1,
		blocked: 1,
	});
	const manifest = (await readFile(join(runRoot, 'manifest.jsonl'), 'utf8'))
		.trim()
		.split('\n')
		.map((line) => JSON.parse(line));
	assert.deepEqual(manifest.map((entry) => entry.status).toSorted(), [
		'blocked',
		'failed',
	]);
	const blocked = manifest.find((entry) => entry.status === 'blocked');
	assert.equal(blocked.reason, 'provider-circuit-open');
	assert.equal(
		blocked.provider_error.provider_code,
		'billing_hard_limit_reached',
	);

	const retryPlan = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1', '2'],
		includeFallback: false,
		asset: 'pair',
	});
	assert.equal(retryPlan.plan.requests.resumable_existing, 0);
	assert.equal(retryPlan.plan.requests.pending, 2);
});

test('max requests caps retry HTTP attempts across concurrent jobs', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const config = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	config.retry.max_attempts = 3;
	await writeFile(fixture.configPath, JSON.stringify(config));
	const provider = await mockProviderSequence([
		{
			status: 500,
			body: {
				error: {
					message: 'temporary provider failure',
					type: 'server_error',
					code: 'server_error',
				},
			},
		},
	]);

	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1', '2'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 2,
			concurrency: 2,
			runId: 'retry-budget',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/2 candidate generation job\(s\) failed/,
	);
	assert.equal(provider.requests.length, 2);
	const journalPaths = (await readdir(fixture.outputRoot, { recursive: true }))
		.filter((path) => path.endsWith('attempt.json'))
		.map((path) => join(fixture.outputRoot, path));
	assert.equal(journalPaths.length, 2);
	const journals = await Promise.all(
		journalPaths.map(async (path) => JSON.parse(await readFile(path, 'utf8'))),
	);
	assert(journals.every((journal) => journal.status === 'failed'));
	assert(
		journals.every((journal) =>
			journal.provider.request_id?.startsWith('req_'),
		),
	);
	const summary = JSON.parse(
		await readFile(
			join(fixture.outputRoot, 'runs', 'retry-budget', 'run.json'),
		),
	);
	assert.equal(summary.provider_attempts, 2);
});

test('fatal quota errors are journaled once and never retried', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const config = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	config.retry.max_attempts = 3;
	await writeFile(fixture.configPath, JSON.stringify(config));
	const provider = await mockProviderSequence([
		{
			status: 429,
			requestId: 'req_fatal_quota',
			body: {
				error: {
					message: 'quota exhausted',
					type: 'insufficient_quota',
					code: 'insufficient_quota',
				},
			},
		},
	]);

	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 3,
			runId: 'fatal-no-retry',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	assert.equal(provider.requests.length, 1);
	const journalPath = join(
		fixture.outputRoot,
		(await readdir(fixture.outputRoot, { recursive: true })).find((path) =>
			path.endsWith('attempt.json'),
		),
	);
	const journal = JSON.parse(await readFile(journalPath, 'utf8'));
	assert.equal(journal.provider.request_id, 'req_fatal_quota');
	assert.equal(journal.error.provider_code, 'insufficient_quota');
	assert.equal(journal.error.batch_fatal, true);
});

test('fatal model parameter errors are not retried', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const config = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	config.retry.max_attempts = 3;
	await writeFile(fixture.configPath, JSON.stringify(config));
	const provider = await mockProviderSequence([
		{
			status: 400,
			body: {
				error: {
					message: 'unsupported model snapshot',
					type: 'invalid_request_error',
					code: 'invalid_value',
					param: 'model',
				},
			},
		},
	]);
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 3,
			runId: 'fatal-model-no-retry',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	assert.equal(provider.requests.length, 1);
});

test('invalid existing receipt state fails closed before a provider call while absent state generates', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const common = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
	};
	const planned = await runCandidateGeneration(common);
	const jobId = planned.plan.jobs[0].job_id;
	const objectRoot = join(
		fixture.outputRoot,
		'objects',
		jobId.slice(0, 2),
		jobId,
	);
	await mkdir(objectRoot, { recursive: true });
	await writeFile(
		join(objectRoot, 'receipt.json'),
		JSON.stringify({ job_id: jobId, status: 'candidate' }),
	);

	await assert.rejects(
		runCandidateGeneration({
			...common,
			execute: true,
			maxRequests: 1,
			runId: 'invalid-preflight',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/Existing receipt.*invalid.*before generation/,
	);
	assert.equal(provider.requests.length, 0);

	await rm(join(objectRoot, 'receipt.json'));
	await writeFile(join(objectRoot, 'raw.png'), pairedImageFixture());
	await assert.rejects(
		runCandidateGeneration({
			...common,
			execute: true,
			maxRequests: 1,
			runId: 'invalid-artifact-preflight',
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/Unrecognized existing artifact state/,
	);
	assert.equal(provider.requests.length, 0);

	await rm(objectRoot, { recursive: true, force: true });
	const generated = await runCandidateGeneration({
		...common,
		execute: true,
		maxRequests: 1,
		runId: 'absent-generates',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	assert.equal(generated.plan.result_counts.generated, 1);
	assert.equal(provider.requests.length, 1);
});

test('journaled paid response recovers locally after interruption and stale lock', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const common = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	};
	const first = await runCandidateGeneration({
		...common,
		execute: true,
		maxRequests: 1,
		runId: 'recovery-source',
	});
	const receiptPath = first.results[0].receipt;
	const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
	const attemptRoot = dirname(receipt.artifact.raw_path);
	const journalPath = join(attemptRoot, 'attempt.json');
	const journalBytes = await readFile(journalPath);
	const journal = JSON.parse(journalBytes.toString('utf8'));
	assert.equal(journal.job_id, receipt.job_id);
	assert.equal(journal.provider.request_id, 'req_test_notebook_art');
	assert.deepEqual(journal.provider.usage, {
		input_tokens: 12,
		output_tokens: 34,
	});
	assert.equal(journal.artifact.raw_sha256, receipt.artifact.raw_sha256);
	assert.equal(
		canonicalSha256(journal.provenance.job_identity),
		journal.job_id,
	);
	for (const name of await readdir(attemptRoot)) {
		if (!['attempt.json', 'provider-response.json', 'raw.png'].includes(name)) {
			await rm(join(attemptRoot, name), { recursive: true, force: true });
		}
	}
	await rm(receiptPath);
	const objectRoot = dirname(receiptPath);
	await writeFile(
		join(objectRoot, 'generation.lock'),
		JSON.stringify({
			schema_version: 1,
			job_id: receipt.job_id,
			pid: 999_999_999,
			hostname: hostname(),
			token: 'interrupted-run',
			created_at: '2026-01-01T00:00:00.000Z',
		}),
	);

	const recovered = await runCandidateGeneration({
		...common,
		execute: true,
		maxRequests: 0,
		runId: 'recovered-without-spend',
	});
	assert.equal(recovered.plan.result_counts.generated, 1);
	assert.equal(recovered.plan.provider_attempts, 0);
	assert.equal(provider.requests.length, 1);
	assert.deepEqual(await readFile(journalPath), journalBytes);
	assert.equal(
		await readFile(join(objectRoot, 'generation.lock')).catch(() => null),
		null,
	);
});

test('timeout retries retain immutable failed and successful attempt journals', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const config = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	config.retry.max_attempts = 2;
	config.retry.request_timeout_ms = 10;
	await writeFile(fixture.configPath, JSON.stringify(config));
	const provider = await mockProviderSequence([
		{ delayMs: 40, buffer: pairedImageFixture() },
		{ buffer: pairedImageFixture(), requestId: 'req_after_timeout' },
	]);
	const result = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 2,
		runId: 'timeout-retry-journal',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	assert.equal(result.plan.provider_attempts, 2);
	assert.equal(provider.requests.length, 2);
	const journalPaths = (await readdir(fixture.outputRoot, { recursive: true }))
		.filter((path) => path.endsWith('attempt.json'))
		.map((path) => join(fixture.outputRoot, path));
	const journals = await Promise.all(
		journalPaths.map(async (path) => JSON.parse(await readFile(path, 'utf8'))),
	);
	assert.deepEqual(journals.map((journal) => journal.status).toSorted(), [
		'failed',
		'response-persisted',
	]);
	const timeout = journals.find((journal) => journal.status === 'failed');
	assert.match(timeout.error.message, /timed out|aborted/i);
	assert.equal(timeout.response, null);
});

test('execute run IDs are immutable even when every job is resumable', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const options = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		runId: 'immutable-run-id',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	};
	const first = await runCandidateGeneration(options);
	assert.equal(first.plan.result_counts.generated, 1);
	await assert.rejects(
		runCandidateGeneration(options),
		/Generation run already exists.*choose a new --run-id/,
	);
	assert.equal(provider.requests.length, 1);
});

test('stable job hashes partition across shards without gaps or duplicates', async () => {
	const fixture = await fixtureFiles();
	const common = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		includeFallback: true,
		asset: 'all',
	};
	const full = await runCandidateGeneration(common);
	const shards = await Promise.all(
		[0, 1, 2].map((shardIndex) =>
			runCandidateGeneration({
				...common,
				shardCount: 3,
				shardIndex,
			}),
		),
	);
	const combined = shards.flatMap(({ plan }) =>
		plan.jobs.map((job) => job.job_id),
	);
	assert.equal(new Set(combined).size, combined.length);
	assert.deepEqual(
		combined.toSorted(),
		full.plan.jobs.map((job) => job.job_id).toSorted(),
	);
});

test('paired mode plans one provider request for both NPC assets', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const result = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
	});
	assert.equal(result.plan.requests.planned, 1);
	assert.equal(result.plan.jobs[0].asset_kind, 'pair');
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'portrait',
		}),
		/always creates portrait and marker together/,
	);
});

test('paired execute splits one response into identity-linked portrait and marker candidates', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(
		pairedImageFixture({
			portrait: { coloredFringe: true },
			marker: { magentaFringe: true },
		}),
	);
	const options = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	};
	const result = await runCandidateGeneration(options);
	assert.equal(result.plan.result_counts.generated, 1);
	assert.equal(provider.requests.length, 1);
	const requestBody = provider.requests[0].body.toString('latin1');
	assert.match(requestBody, /Bridget pair prompt/);
	assert.match(requestBody, /left portrait and right marker, same person/);
	assert.match(requestBody, /Left-cell portrait contract: flat key portrait/);
	assert.match(
		requestBody,
		/Right-cell marker contract: flat key character-only marker, empty hands, no scenery/,
	);
	assert.match(requestBody, /exactly two depictions of one character/);
	assert.match(requestBody, /portrait-style\.png/);
	assert.match(requestBody, /notebook-concept\.png/);

	const receipt = JSON.parse(await readFile(result.results[0].receipt, 'utf8'));
	assert.equal(receipt.receipt_type, 'notebook-person-art-pair-candidate');
	assert.equal(receipt.asset.kind, 'pair');
	assert.equal(receipt.identity_lock.generation, 'single-provider-request');
	assert.equal(receipt.identity_lock.status, 'pending-human-review');
	assert.equal(receipt.provider.request_id, 'req_test_notebook_art');
	for (const kind of ['portrait', 'marker']) {
		assert(await readFile(receipt.artifact.children[kind].raw_path));
		assert(await readFile(receipt.artifact.children[kind].candidate_path));
		assert.equal(receipt.artifact.children[kind].width, 64);
		assert.equal(receipt.artifact.children[kind].height, 64);
	}
	assert.equal(
		receipt.artifact.children.portrait.candidate_validation
			.normalized_ink_color,
		'#36362e',
	);
	assert(
		receipt.artifact.children.portrait.raw_validation.colored_subject_fraction >
			0.02,
		'thin chromatic ink fringes should exercise the relaxed edge ceiling',
	);
	assert(
		receipt.artifact.children.portrait.raw_validation.colored_fill_fraction <
			0.003,
		'thin chromatic ink fringes must not look like solid painted fill',
	);
	assert.equal(
		receipt.artifact.children.marker.candidate_validation.normalized_ink_color,
		null,
	);
	assert.equal(
		receipt.artifact.children.marker.candidate_validation
			.residual_key_spill_fraction,
		0,
		'opaque magenta-balanced edge pixels must be despilled',
	);
	const markerCandidate = PNG.sync.read(
		await readFile(receipt.artifact.children.marker.candidate_path),
	);
	const despilledOffset = (30 * markerCandidate.width + 19) * 4;
	assert.equal(
		markerCandidate.data[despilledOffset],
		markerCandidate.data[despilledOffset + 1],
		'despilled marker edges should be neutral rather than purple',
	);

	const resumed = await runCandidateGeneration({
		...options,
		maxRequests: 0,
		environment: {},
	});
	assert.equal(resumed.plan.result_counts.resume_skip, 1);
	assert.equal(provider.requests.length, 1);
});

test('paired execute normalizes complete oversized subjects without another provider call', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const config = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	config.validation.asset_contracts.marker.max_subject_bounds_height_fraction = 0.65;
	await writeFile(fixture.configPath, JSON.stringify(config));
	const provider = await mockProvider(
		pairedImageFixture({
			portrait: { oversized: true },
			marker: {
				magentaFringe: true,
				oversized: true,
				nearKeyCorner: true,
			},
		}),
	);
	const result = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	assert.equal(result.plan.result_counts.generated, 1);
	assert.equal(provider.requests.length, 1);
	const receipt = JSON.parse(await readFile(result.results[0].receipt, 'utf8'));
	const portraitFraming =
		receipt.artifact.children.portrait.candidate_validation
			.framing_normalization;
	const markerFraming =
		receipt.artifact.children.marker.candidate_validation.framing_normalization;
	assert.equal(portraitFraming.applied, true);
	assert.equal(markerFraming.applied, true);
	assert(portraitFraming.after.height_fraction <= 0.7);
	assert(markerFraming.after.height_fraction <= 0.65);
	assert(portraitFraming.after.margin_fraction > 0.1);
	assert(markerFraming.after.margin_fraction > 0.1);
	assert.equal(
		receipt.artifact.children.marker.candidate_validation
			.residual_key_spill_fraction,
		0,
	);
});

test('paired execute normalizes a complete low-margin figure without treating it as cropped', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(
		pairedImageFixture({ marker: { lowMargin: true } }),
	);
	const result = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});

	assert.equal(result.plan.result_counts.generated, 1);
	const receipt = JSON.parse(await readFile(result.results[0].receipt, 'utf8'));
	const marker = receipt.artifact.children.marker;
	assert(marker.raw_validation.subject_margin_fraction < 0.05);
	assert(marker.raw_validation.subject_margin_pixels >= 2);
	assert.equal(marker.candidate_validation.framing_normalization.applied, true);
	assert(
		marker.candidate_validation.framing_normalization.after.margin_fraction >=
			0.05,
	);
});

test('paired execute ignores near-key provider drift at a cell border when computing subject bounds', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(
		pairedImageFixture({ marker: { nearKeyBottomBorder: true } }),
	);
	const result = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});

	assert.equal(result.plan.result_counts.generated, 1);
	const receipt = JSON.parse(await readFile(result.results[0].receipt, 'utf8'));
	const marker = receipt.artifact.children.marker;
	assert.equal(marker.raw_validation.bounds_key_distance, 136);
	assert(marker.raw_validation.subject_margin_pixels > 0);
});

test('paired execute rejects and preserves a marker cropped by its cell boundary', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(
		pairedImageFixture({ marker: { touchBottom: true } }),
	);
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failurePath = files.find((path) => path.endsWith('failure.json'));
	assert(failurePath);
	const failure = JSON.parse(
		await readFile(join(fixture.outputRoot, failurePath), 'utf8'),
	);
	assert.match(
		failure.error.message,
		/Marker cell failed validation: .*subject margin/,
	);
	assert.equal(failure.artifact.raw_persisted, true);
	assert.equal(failure.artifact.children.marker.raw_persisted, true);
});

test('paired execute identifies and preserves a portrait contract failure', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(
		pairedImageFixture({ portrait: { dense: true } }),
	);
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'pair',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failurePath = files.find((path) => path.endsWith('failure.json'));
	assert(failurePath);
	const failure = JSON.parse(
		await readFile(join(fixture.outputRoot, failurePath), 'utf8'),
	);
	assert.match(
		failure.error.message,
		/Portrait cell failed validation: .*(subject coverage|dark ink coverage)/,
	);
	assert.equal(failure.artifact.raw_persisted, true);
	assert.equal(failure.artifact.children.portrait.raw_persisted, true);
});

test('execute writes validated pending candidates and resumes without another call', async () => {
	const fixture = await fixtureFiles();
	const provider = await mockProvider(imageFixture());
	const environment = {
		OPENAI_API_KEY: 'test-secret-that-must-not-be-recorded',
		RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
	};
	const options = {
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'portrait',
		execute: true,
		maxRequests: 1,
		environment,
	};
	const first = await runCandidateGeneration(options);
	assert.equal(first.plan.result_counts.generated, 1);
	assert.equal(provider.requests.length, 1);
	const requestBody = provider.requests[0].body.toString('latin1');
	assert.equal(provider.requests[0].url, '/v1/images/edits');
	assert.match(requestBody, /name="model"/);
	assert.match(requestBody, /gpt-image-2-test-snapshot/);
	assert.match(requestBody, /name="prompt"/);
	assert.match(requestBody, /Bridget portrait prompt/);
	assert.match(requestBody, /name="image\[\]"/);
	assert.match(requestBody, /portrait-style\.png/);
	assert.doesNotMatch(requestBody, /notebook-concept\.png/);

	const receiptPath = first.results[0].receipt;
	const receiptText = await readFile(receiptPath, 'utf8');
	const receipt = JSON.parse(receiptText);
	assert.equal(receipt.status, 'candidate');
	assert.equal(receipt.review.status, 'pending');
	assert.equal(receipt.promotion.eligible, false);
	assert.equal(receipt.provider.request_id, 'req_test_notebook_art');
	assert.equal(receipt.subject.npc_id, 1);
	assert.deepEqual(
		receipt.provenance.reference_inputs.map((reference) => reference.id),
		['portrait-style'],
	);
	assert(!receiptText.includes(environment.OPENAI_API_KEY));
	const candidate = PNG.sync.read(
		await readFile(receipt.artifact.candidate_path),
	);
	assert.equal(
		candidate.data[3],
		0,
		'top-left key pixel should be transparent',
	);
	const subjectOffset = (30 * candidate.width + 30) * 4;
	assert(candidate.data[subjectOffset + 3] > 32, 'subject must remain visible');
	assert.deepEqual(
		Array.from(candidate.data.subarray(subjectOffset, subjectOffset + 3)),
		[54, 54, 46],
		'portrait postprocessing should normalize retained strokes to graphite ink',
	);

	const resumed = await runCandidateGeneration({
		...options,
		maxRequests: 0,
		environment: {},
	});
	assert.equal(resumed.plan.result_counts.resume_skip, 1);
	assert.equal(
		provider.requests.length,
		1,
		'resume must not call the provider',
	);
});

test('execute rejects a densely rendered portrait that violates the sketch contract', async () => {
	const fixture = await fixtureFiles();
	const provider = await mockProvider(imageFixture({ dense: true }));
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'portrait',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failurePath = files.find((path) => path.endsWith('failure.json'));
	assert(failurePath);
	const failure = JSON.parse(
		await readFile(join(fixture.outputRoot, failurePath), 'utf8'),
	);
	assert.match(
		failure.error.message,
		/subject coverage|inked drawing height|dark ink coverage|ink density/,
	);
	assert.equal(failure.artifact.raw_persisted, true);
});

test('execute rejects real portrait color while allowing keyed edge antialiasing', async () => {
	const fixture = await fixtureFiles();
	const provider = await mockProvider(imageFixture({ coloredFill: true }));
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'portrait',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failurePath = files.find((path) => path.endsWith('failure.json'));
	assert(failurePath);
	const failure = JSON.parse(
		await readFile(join(fixture.outputRoot, failurePath), 'utf8'),
	);
	assert.match(failure.error.message, /solid colored fill coverage/);
});

test('execute rejects key-only provider output as a blank candidate', async () => {
	const fixture = await fixtureFiles();
	const provider = await mockProvider(imageFixture({ blank: true }));
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'marker',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failure = files.find((path) => path.endsWith('failure.json'));
	assert(failure, 'failed content validation should write a failure receipt');
	const failureBody = JSON.parse(
		await readFile(join(fixture.outputRoot, failure), 'utf8'),
	);
	assert.match(failureBody.error.message, /blank\/degenerate/);
	assert.equal(failureBody.provider.request_id, 'req_test_notebook_art');
	assert.equal(failureBody.artifact.raw_persisted, true);
	assert.equal(
		failureBody.artifact.raw_sha256,
		failureBody.artifact.raw_sha256.toLowerCase(),
	);
	assert.equal(
		(await readFile(failureBody.artifact.raw_path)).toString('hex'),
		imageFixture({ blank: true }).toString('hex'),
	);
});

test('reuses preserved provider output across a local-only postprocess revision', async () => {
	const fixture = await fixtureFiles();
	const initialConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	initialConfig.validation.key_feather_distance = 20;
	initialConfig.raw_output.postprocess_revision = 'test-key-v1';
	await writeFile(fixture.configPath, JSON.stringify(initialConfig));
	const provider = await mockProvider(imageFixture({ nearKeyCorner: true }));
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'portrait',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const relativeFailurePath = files.find((path) =>
		path.endsWith('failure.json'),
	);
	assert(relativeFailurePath);
	const failurePath = join(fixture.outputRoot, relativeFailurePath);
	const failure = JSON.parse(await readFile(failurePath, 'utf8'));
	assert.equal(
		canonicalSha256(failure.provenance.job_identity),
		failure.job_id,
	);
	assert.equal(failure.provenance.job_identity_sha256, failure.job_id);

	const revisedConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	revisedConfig.validation.key_feather_distance = 136;
	revisedConfig.raw_output.postprocess_revision = 'test-key-v2';
	await writeFile(fixture.configPath, JSON.stringify(revisedConfig));
	const result = await reprocessCandidateFailure({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		failurePath,
	});

	assert.equal(result.status, 'reprocessed');
	assert.notEqual(result.receipt.job_id, failure.job_id);
	assert.equal(result.receipt.reprocessing.source_job_id, failure.job_id);
	assert.equal(
		result.receipt.reprocessing.source_failure_sha256,
		createHash('sha256')
			.update(await readFile(failurePath))
			.digest('hex'),
	);
	assert.equal(result.receipt.reprocessing.provider_request_reused, true);
	assert.equal(result.receipt.provider.request_id, 'req_test_notebook_art');
	assert.equal(
		canonicalSha256(result.receipt.provenance.job_identity),
		result.receipt.job_id,
	);
	assert.equal(
		provider.requests.length,
		1,
		'local reprocessing must not call the provider',
	);
});

test('reprocesses a successful candidate receipt under a new postprocess revision', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const generated = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		runId: 'source-receipt-run',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	const sourceReceiptPath = generated.results[0].receipt;
	const sourceReceipt = JSON.parse(await readFile(sourceReceiptPath, 'utf8'));
	assert.equal(
		canonicalSha256(sourceReceipt.provenance.job_identity),
		sourceReceipt.job_id,
	);
	const revisedConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	revisedConfig.pipeline_revision = 'test-v2';
	revisedConfig.raw_output.postprocess_revision = 'test-key-v3';
	revisedConfig.raw_output.framing_normalization.headroom_fraction = 0.03;
	await writeFile(fixture.configPath, JSON.stringify(revisedConfig));

	const result = await reprocessCandidateReceipt({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		receiptPath: sourceReceiptPath,
		runId: 'reprocessed-receipt-run',
	});

	assert.equal(result.status, 'reprocessed');
	assert.notEqual(result.receipt.job_id, sourceReceipt.job_id);
	assert.equal(result.receipt.reprocessing.source_job_id, sourceReceipt.job_id);
	assert.equal(
		result.receipt.reprocessing.source_receipt_sha256,
		createHash('sha256')
			.update(await readFile(sourceReceiptPath))
			.digest('hex'),
	);
	assert.equal(result.receipt.reprocessing.provider_request_reused, true);
	assert.equal(result.receipt.run_id, 'reprocessed-receipt-run');
	assert.equal(
		canonicalSha256(result.receipt.provenance.job_identity),
		result.receipt.job_id,
	);
	assert.equal(provider.requests.length, 1);
});

test('rejects legacy changed-job receipts without canonical identity', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const generated = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	const receiptPath = generated.results[0].receipt;
	const legacyReceipt = JSON.parse(await readFile(receiptPath, 'utf8'));
	delete legacyReceipt.provenance.job_identity;
	delete legacyReceipt.provenance.job_identity_sha256;
	await writeFile(receiptPath, JSON.stringify(legacyReceipt));

	const revisedConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	revisedConfig.pipeline_revision = 'test-v2';
	revisedConfig.raw_output.postprocess_revision = 'test-key-v3';
	await writeFile(fixture.configPath, JSON.stringify(revisedConfig));

	await assert.rejects(
		reprocessCandidateReceipt({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			receiptPath,
		}),
		/Legacy source receipt lacks canonical job_identity/,
	);
});

test('rejects altered same-job receipt metadata and bound bytes', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const generated = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 1,
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	const receiptPath = generated.results[0].receipt;
	const originalReceiptBytes = await readFile(receiptPath);
	const originalReceipt = JSON.parse(originalReceiptBytes.toString('utf8'));
	const promptPath = originalReceipt.provenance.prompt_path;
	const inputRecordPath = originalReceipt.provenance.input_record_path;
	const rawPath = originalReceipt.artifact.raw_path;
	const originalPrompt = await readFile(promptPath);
	const originalInputRecord = await readFile(inputRecordPath);
	const originalRaw = await readFile(rawPath);
	const originalReference = await readFile(fixture.referencePath);
	const legacyCurrentReceipt = structuredClone(originalReceipt);
	delete legacyCurrentReceipt.provenance.job_identity;
	delete legacyCurrentReceipt.provenance.job_identity_sha256;
	await writeFile(receiptPath, JSON.stringify(legacyCurrentReceipt));
	const legacyExact = await reprocessCandidateReceipt({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		receiptPath,
	});
	assert.equal(legacyExact.status, 'resume-skip');
	await writeFile(receiptPath, originalReceiptBytes);

	const exact = await reprocessCandidateReceipt({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		receiptPath,
	});
	assert.equal(exact.status, 'resume-skip');

	const cases = [
		{
			name: 'canonical identity payload',
			mutate: async (receipt) => {
				receipt.provenance.job_identity.pipeline_revision = 'fabricated-v99';
			},
		},
		{
			name: 'subject metadata',
			mutate: async (receipt) => {
				receipt.subject.name = 'Invented Person';
			},
		},
		{
			name: 'provider metadata',
			mutate: async (receipt) => {
				receipt.provider.request.quality = 'low';
			},
		},
		{
			name: 'config provenance',
			mutate: async (receipt) => {
				receipt.provenance.config_sha256 = '0'.repeat(64);
			},
		},
		{
			name: 'prompt bytes and matching metadata hash',
			mutate: async (receipt) => {
				const bytes = Buffer.from('fabricated but self-consistent prompt\n');
				await writeFile(promptPath, bytes);
				receipt.provenance.prompt_sha256 = createHash('sha256')
					.update(bytes.subarray(0, bytes.length - 1))
					.digest('hex');
			},
		},
		{
			name: 'input bytes and matching metadata hash',
			mutate: async (receipt) => {
				const record = JSON.parse(originalInputRecord.toString('utf8'));
				record.name = 'Invented Person';
				await writeFile(inputRecordPath, JSON.stringify(record));
				receipt.subject.input_record_sha256 = createHash('sha256')
					.update(JSON.stringify(record))
					.digest('hex');
			},
		},
		{
			name: 'reference bytes and matching metadata hash',
			mutate: async (receipt) => {
				const bytes = imageFixture({ coloredFringe: true });
				await writeFile(fixture.referencePath, bytes);
				receipt.provenance.reference_inputs[0].sha256 = createHash('sha256')
					.update(bytes)
					.digest('hex');
			},
		},
		{
			name: 'raw bytes and matching top-level hash',
			mutate: async (receipt) => {
				const bytes = pairedImageFixture({
					portrait: { coloredFringe: true },
				});
				await writeFile(rawPath, bytes);
				receipt.artifact.raw_sha256 = createHash('sha256')
					.update(bytes)
					.digest('hex');
			},
		},
	];

	for (const adversarial of cases) {
		const receipt = structuredClone(originalReceipt);
		await adversarial.mutate(receipt);
		await writeFile(receiptPath, JSON.stringify(receipt));
		await assert.rejects(
			reprocessCandidateReceipt({
				configPath: fixture.configPath,
				inputsPath: fixture.inputsPath,
				outputRoot: fixture.outputRoot,
				receiptPath,
			}),
			undefined,
			adversarial.name,
		);
		await Promise.all([
			writeFile(receiptPath, originalReceiptBytes),
			writeFile(promptPath, originalPrompt),
			writeFile(inputRecordPath, originalInputRecord),
			writeFile(rawPath, originalRaw),
			writeFile(fixture.referencePath, originalReference),
		]);
	}
});

test('rejects a failure that claims the unchanged current job identity', async () => {
	const fixture = await fixtureFiles();
	const provider = await mockProvider(imageFixture({ blank: true }));
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'portrait',
			execute: true,
			maxRequests: 1,
			environment: {
				OPENAI_API_KEY: 'test-key',
				RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
			},
		}),
		/1 candidate generation job\(s\) failed/,
	);
	const files = await readdir(fixture.outputRoot, { recursive: true });
	const failurePath = join(
		fixture.outputRoot,
		files.find((path) => path.endsWith('failure.json')),
	);
	await assert.rejects(
		reprocessCandidateFailure({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			failurePath,
		}),
		/only be reprocessed into a changed content-addressed job/,
	);
});

test('reprocesses a fallback receipt without selecting a named NPC job', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	const generated = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: true,
		asset: 'pair',
		execute: true,
		maxRequests: 2,
		runId: 'source-fallback-receipt-run',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	const receipts = await Promise.all(
		generated.results.map(async ({ receipt }) => ({
			path: receipt,
			value: JSON.parse(await readFile(receipt, 'utf8')),
		})),
	);
	const sourceFallback = receipts.find(
		({ value }) => value.subject.kind === 'fallback',
	);
	assert(sourceFallback, 'expected generated fallback receipt');

	const revisedConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	revisedConfig.pipeline_revision = 'test-v2';
	revisedConfig.raw_output.postprocess_revision = 'test-key-v3';
	await writeFile(fixture.configPath, JSON.stringify(revisedConfig));

	const result = await reprocessCandidateReceipt({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		receiptPath: sourceFallback.path,
		runId: 'reprocessed-fallback-receipt-run',
	});

	assert.equal(result.status, 'reprocessed');
	assert.notEqual(result.receipt.job_id, sourceFallback.value.job_id);
	assert.equal(result.receipt.subject.kind, 'fallback');
	assert.equal(result.receipt.subject.npc_id, null);
	assert.equal(
		result.receipt.reprocessing.source_job_id,
		sourceFallback.value.job_id,
	);
	assert.equal(result.receipt.reprocessing.provider_request_reused, true);
	assert.equal(provider.requests.length, 2);
});

test('reprocesses a generation manifest locally without provider requests', async () => {
	const fixture = await fixtureFiles({ paired: true });
	const provider = await mockProvider(pairedImageFixture());
	await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		includeFallback: false,
		asset: 'pair',
		execute: true,
		maxRequests: 2,
		runId: 'source-manifest-run',
		environment: {
			OPENAI_API_KEY: 'test-key',
			RUNDALE_ART_OPENAI_BASE_URL: provider.baseUrl,
		},
	});
	const revisedConfig = JSON.parse(await readFile(fixture.configPath, 'utf8'));
	revisedConfig.pipeline_revision = 'test-v2';
	revisedConfig.raw_output.postprocess_revision = 'test-key-v3';
	await writeFile(fixture.configPath, JSON.stringify(revisedConfig));
	const result = await reprocessCandidateManifest({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		manifestPath: join(
			fixture.outputRoot,
			'runs',
			'source-manifest-run',
			'manifest.jsonl',
		),
		runId: 'reprocessed-manifest-run',
	});

	assert.equal(result.plan.status, 'completed');
	assert.equal(result.plan.provider_requests, 0);
	assert.equal(result.plan.result_counts.reprocessed, 2);
	assert.equal(provider.requests.length, 2);
	assert(
		await readFile(
			join(
				fixture.outputRoot,
				'runs',
				'reprocessed-manifest-run',
				'manifest.jsonl',
			),
			'utf8',
		),
	);
});

test('execute refuses a batch larger than the explicit request cap', async () => {
	const fixture = await fixtureFiles();
	await assert.rejects(
		runCandidateGeneration({
			configPath: fixture.configPath,
			inputsPath: fixture.inputsPath,
			outputRoot: fixture.outputRoot,
			npcIds: ['1'],
			includeFallback: false,
			asset: 'all',
			execute: true,
			maxRequests: 1,
			environment: { OPENAI_API_KEY: 'test-key' },
		}),
		/exceeding --max-requests 1/,
	);
});
