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
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PNG } from 'pngjs';
import { afterEach, test } from 'vitest';

import {
	reprocessCandidateFailure,
	runCandidateGeneration,
} from './generate-notebook-person-candidates.mjs';

const cleanupPaths = [];
const cleanupServers = [];

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
			const bottom = touchBottom ? 63 : 46;
			const inBounds = x >= 20 && x <= 43 && y >= 17 && y <= bottom;
			const sparseInk =
				inBounds &&
				(x === 20 ||
					x === 43 ||
					y === 17 ||
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
				pair_contract: 'left portrait and right marker, same person',
				portrait_contract: 'flat key portrait',
				marker_contract: 'flat key marker',
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
			schema_version: 1,
			fallback: {
				pair_prompt: 'Unknown pair prompt',
				portrait_prompt: 'Unknown portrait prompt',
				marker_prompt: 'Unknown marker prompt',
				art_direction: { fallback: true },
			},
			npcs: [
				{
					npc_id: 1,
					name: 'Bridget Test',
					pair_prompt: 'Bridget pair prompt',
					portrait_prompt: 'Bridget portrait prompt',
					marker_prompt: 'Bridget marker prompt',
					art_direction: { identity: 'bridget' },
				},
				{
					npc_id: 2,
					name: 'Cormac Test',
					pair_prompt: 'Cormac pair prompt',
					portrait_prompt: 'Cormac portrait prompt',
					marker_prompt: 'Cormac marker prompt',
					art_direction: { identity: 'cormac' },
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
	assert.match(requestBody, /Right-cell marker contract: flat key marker/);
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
		/Portrait cell failed validation: .*subject coverage/,
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

test('reprocesses a preserved raw response locally after validation improves', async () => {
	const fixture = await fixtureFiles();
	const plan = await runCandidateGeneration({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		npcIds: ['1'],
		includeFallback: false,
		asset: 'portrait',
	});
	const jobId = plan.plan.jobs[0].job_id;
	const objectRoot = join(
		fixture.outputRoot,
		'objects',
		jobId.slice(0, 2),
		jobId,
	);
	const attemptRoot = join(objectRoot, 'attempts', 'preserved-test-attempt');
	const rawPath = join(attemptRoot, 'raw.png');
	const failurePath = join(attemptRoot, 'failure.json');
	const raw = imageFixture({ nearKeyCorner: true });
	await mkdir(attemptRoot, { recursive: true });
	await Promise.all([
		writeFile(rawPath, raw),
		writeFile(join(objectRoot, 'prompt.txt'), 'Bridget portrait prompt\n'),
		writeFile(join(objectRoot, 'input-record.json'), '{}\n'),
	]);
	await writeFile(
		failurePath,
		JSON.stringify({
			schema_version: 1,
			receipt_type: 'notebook-person-art-generation-failure',
			job_id: jobId,
			run_id: 'original-failed-run',
			status: 'failed',
			subject: { kind: 'npc', npc_id: 1, name: 'Bridget Test' },
			asset_kind: 'portrait',
			candidate_index: 1,
			provider: {
				id: 'openai',
				adapter: 'openai-images-edits-v1',
				base_url: 'https://api.openai.com/v1',
				model: 'gpt-image-2-test-snapshot',
				endpoint: '/images/edits',
				request: { size: '64x64' },
				request_id: 'req_preserved',
				attempts: 1,
				usage: null,
			},
			artifact: {
				raw_path: rawPath,
				raw_sha256: createHash('sha256').update(raw).digest('hex'),
				raw_persisted: true,
				media_type: 'image/png',
			},
			error: { message: 'old corner threshold rejected this image' },
		}),
	);

	const result = await reprocessCandidateFailure({
		configPath: fixture.configPath,
		inputsPath: fixture.inputsPath,
		outputRoot: fixture.outputRoot,
		failurePath,
	});
	assert.equal(result.status, 'reprocessed');
	assert.equal(result.receipt.provider.request_id, 'req_preserved');
	assert.equal(result.receipt.review.status, 'pending');
	assert.equal(result.receipt.reprocessing.source_failure_path, failurePath);
	assert.equal(result.receipt.artifact.raw_validation.keyed_corners, 4);
	assert(await readFile(result.receipt.artifact.candidate_path));
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
	assert.equal(result.receipt.reprocessing.provider_request_reused, true);
	assert.equal(result.receipt.provider.request_id, 'req_test_notebook_art');
	assert.equal(
		provider.requests.length,
		1,
		'local reprocessing must not call the provider',
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
