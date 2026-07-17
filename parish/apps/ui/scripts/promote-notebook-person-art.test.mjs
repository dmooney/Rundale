// @vitest-environment node

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { afterEach, test } from 'vitest';

import {
	helpText,
	promoteNotebookPersonArt,
	REQUIRED_PAIR_REVIEW_CHECKS,
} from './promote-notebook-person-art.mjs';
import {
	hairTopologyBinding,
	markerIdentityBinding,
} from './notebook-person-art-approval-contract.mjs';

const cleanupPaths = [];
const png = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+8jHqWQAAAABJRU5ErkJggg==',
	'base64',
);

afterEach(async () => {
	await Promise.all(
		cleanupPaths
			.splice(0)
			.map((path) => rm(path, { recursive: true, force: true })),
	);
});

function hash(value) {
	return createHash('sha256').update(value).digest('hex');
}

function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) {
		return `[${value.map((child) => canonicalJson(child)).join(',')}]`;
	}
	return `{${Object.entries(value)
		.toSorted(([left], [right]) => left.localeCompare(right))
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

function pretty(value) {
	return `${JSON.stringify(value, null, '\t')}\n`;
}

async function write(path, value) {
	await mkdir(dirname(path), { recursive: true });
	await writeFile(path, value);
}

async function writeJson(path, value) {
	await write(path, pretty(value));
}

function reviewId(base) {
	return hash(JSON.stringify(base)).slice(0, 24);
}

function hairTopology(seed) {
	return {
		color_and_texture: `${seed} texture`,
		front: { family: `${seed}-front`, description: `${seed} front` },
		rear: { family: `${seed}-rear`, description: `${seed} rear` },
		covering: {
			family: `${seed}-covering`,
			description: `${seed} covering`,
		},
		silhouette: {
			family: `${seed}-silhouette`,
			description: `${seed} silhouette`,
		},
		loose_details: `${seed} loose details`,
	};
}

function markerIdentity(seed) {
	return {
		composition: 'character-only',
		silhouette: `${seed} silhouette; no object/scenery`,
		stance: `${seed} empty-handed stance; no object/scenery`,
		empty_hand_pose: `${seed}-hands-at-sides`,
		readability_cues: [
			{ kind: 'body-shape', description: `${seed} build` },
			{ kind: 'hair-or-headwear', description: `${seed} hair` },
			{ kind: 'clothing', description: `${seed} clothing` },
		],
		tiny_readability_notes: [`Keep ${seed} readable at scene size.`],
	};
}

async function fixture({ decision = 'approved', npcId = 7 } = {}) {
	const root = await mkdtemp(join(tmpdir(), 'rundale-art-promotion-'));
	cleanupPaths.push(root);
	const objectRoot = join(root, 'candidates', 'objects', 'aa', 'job');
	const attemptRoot = join(objectRoot, 'attempts', 'attempt-1');
	const receiptPath = join(objectRoot, 'receipt.json');
	const decisionPath = join(attemptRoot, 'reviews', 'decision.json');
	const pointerPath = join(objectRoot, 'review.json');
	const packetRoot = join(root, 'review-packet');
	const packetPath = join(packetRoot, 'manifest.json');
	const castReviewPath = join(root, 'whole-cast-review.json');
	const templatePath = join(packetRoot, 'pair-review.json');
	const configPath = join(root, 'generation-config.json');
	const inputsPath = join(root, 'npc-art-inputs.json');
	const artDirectionPath = join(root, 'npc-art-direction-v1.json');
	const referencePath = join(root, 'reference.png');
	const promptPath = join(objectRoot, 'prompt.txt');
	const inputRecordPath = join(objectRoot, 'input-record.json');
	const providerRawPath = join(attemptRoot, 'raw.png');
	const portraitRawPath = join(attemptRoot, 'portrait-raw.png');
	const portraitPath = join(attemptRoot, 'portrait-candidate.png');
	const markerRawPath = join(attemptRoot, 'marker-raw.png');
	const markerPath = join(attemptRoot, 'marker-candidate.png');

	const inputRecord = {
		npc_id: npcId,
		name: 'Synthetic Review Subject',
		pair_prompt: 'Identity locked pair.',
		portrait_prompt: 'Sparse portrait.',
		marker_prompt: 'Tiny marker.',
	};
	const inputs = {
		schema_version: 1,
		fallback: {
			pair_prompt: 'Fallback pair.',
			portrait_prompt: 'Fallback portrait.',
			marker_prompt: 'Fallback marker.',
		},
		npcs: [inputRecord],
	};
	const config = {
		schema_version: 1,
		pipeline_revision: 'fixture-pairs-v1',
		provider: {
			id: 'fixture-provider',
			adapter: 'fixture-adapter',
			model: 'fixture-model-v1',
			endpoint: '/fixture',
			request: { size: '2x1', output_format: 'png' },
		},
		reference_inputs: [
			{
				id: 'fixture-style',
				path: referencePath,
				purpose: 'Synthetic style fixture only.',
				asset_kinds: ['pair'],
			},
		],
		raw_output: { postprocess_revision: 'fixture-v1' },
		validation: { fixture: true },
	};
	const artDirection = {
		schema_version: 4,
		npcs: [
			{
				npc_id: npcId,
				portrait_identity: { hair_topology: hairTopology('subject') },
				marker_identity: markerIdentity('subject'),
			},
			{
				npc_id: npcId + 100,
				portrait_identity: { hair_topology: hairTopology('unrelated') },
				marker_identity: markerIdentity('unrelated'),
			},
		],
		fallback: {
			portrait_identity: { hair_topology: hairTopology('fallback') },
			marker_identity: markerIdentity('fallback'),
		},
	};
	const prompt = 'Synthetic provider prompt';
	await Promise.all([
		writeJson(configPath, config),
		writeJson(inputsPath, inputs),
		writeJson(artDirectionPath, artDirection),
		write(referencePath, png),
		write(promptPath, `${prompt}\n`),
		writeJson(inputRecordPath, inputRecord),
		write(providerRawPath, Buffer.concat([png, Buffer.from('sheet')])),
		write(portraitRawPath, Buffer.concat([png, Buffer.from('portrait-raw')])),
		write(portraitPath, Buffer.concat([png, Buffer.from('portrait-master')])),
		write(markerRawPath, Buffer.concat([png, Buffer.from('marker-raw')])),
		write(markerPath, Buffer.concat([png, Buffer.from('marker-master')])),
	]);

	const files = Object.fromEntries(
		await Promise.all(
			Object.entries({
				config: configPath,
				inputs: inputsPath,
				reference: referencePath,
				prompt: promptPath,
				inputRecord: inputRecordPath,
				providerRaw: providerRawPath,
				portraitRaw: portraitRawPath,
				portrait: portraitPath,
				markerRaw: markerRawPath,
				marker: markerPath,
			}).map(async ([key, path]) => [
				key,
				{ path, bytes: await readFile(path) },
			]),
		),
	);
	for (const file of Object.values(files)) file.sha256 = hash(file.bytes);

	const inputRecordSha256 = hash(canonicalJson(inputRecord));
	const promptSha256 = hash(prompt);
	const referenceInputs = [
		{
			id: 'fixture-style',
			path: referencePath,
			purpose: 'Synthetic style fixture only.',
			sha256: files.reference.sha256,
		},
	];
	const provider = {
		id: config.provider.id,
		adapter: config.provider.adapter,
		base_url: 'https://fixture.invalid/v1',
		model: config.provider.model,
		endpoint: config.provider.endpoint,
		request: config.provider.request,
		request_id: 'fixture-request-id',
		provider_created_at: 1,
		attempts: 1,
		usage: { total_tokens: 0 },
	};
	const identity = {
		schema_version: 1,
		pipeline_revision: config.pipeline_revision,
		provider: {
			id: config.provider.id,
			adapter: config.provider.adapter,
			model: config.provider.model,
			request: config.provider.request,
		},
		raw_output: config.raw_output,
		validation: config.validation,
		reference_inputs: referenceInputs.map(({ id, sha256 }) => ({ id, sha256 })),
		subject_kind: 'npc',
		npc_id: npcId,
		input_record_sha256: inputRecordSha256,
		asset_kind: 'pair',
		candidate_index: 1,
		prompt_sha256: promptSha256,
	};
	const subject = {
		kind: 'npc',
		npc_id: npcId,
		name: inputRecord.name,
		input_record_sha256: inputRecordSha256,
	};
	const asset = {
		kind: 'pair',
		candidate_index: 1,
		children: ['portrait', 'marker'],
	};
	const children = {
		portrait: {
			raw_path: portraitRawPath,
			raw_sha256: files.portraitRaw.sha256,
			candidate_path: portraitPath,
			candidate_sha256: files.portrait.sha256,
			media_type: 'image/png',
			width: 1,
			height: 1,
			raw_validation: { fixture: true },
			candidate_validation: { fixture: true },
		},
		marker: {
			raw_path: markerRawPath,
			raw_sha256: files.markerRaw.sha256,
			candidate_path: markerPath,
			candidate_sha256: files.marker.sha256,
			media_type: 'image/png',
			width: 1,
			height: 1,
			raw_validation: { fixture: true },
			candidate_validation: { fixture: true },
		},
	};
	const receipt = {
		schema_version: 1,
		receipt_type: 'notebook-person-art-pair-candidate',
		job_id: hash(canonicalJson(identity)),
		status: 'candidate',
		review: {
			status: 'pending',
			reviewer: null,
			reviewed_at: null,
			notes: null,
		},
		promotion: { eligible: false, reason: 'Atomic human review required' },
		identity_lock: {
			generation: 'single-provider-request',
			status: 'pending-human-review',
		},
		subject,
		asset,
		provider,
		provenance: {
			config_path: configPath,
			config_sha256: files.config.sha256,
			inputs_path: inputsPath,
			inputs_sha256: files.inputs.sha256,
			prompt_path: promptPath,
			prompt_sha256: promptSha256,
			input_record_path: inputRecordPath,
			reference_inputs: referenceInputs,
		},
		artifact: {
			raw_path: providerRawPath,
			raw_sha256: files.providerRaw.sha256,
			media_type: 'image/png',
			width: 2,
			height: 1,
			children,
		},
		reprocessing: null,
		timing: {
			started_at: '2026-07-12T00:00:00.000Z',
			completed_at: '2026-07-12T00:00:01.000Z',
		},
	};
	await writeJson(receiptPath, receipt);
	const receiptBytes = await readFile(receiptPath);
	const candidateSha256 = hash(
		JSON.stringify({
			portrait: children.portrait.candidate_sha256,
			marker: children.marker.candidate_sha256,
		}),
	);
	const approved = decision === 'approved';
	const decisionBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-human-review-decision',
		candidate_receipt_path: receiptPath,
		candidate_receipt_sha256: hash(receiptBytes),
		candidate_sha256: candidateSha256,
		raw_sha256: files.providerRaw.sha256,
		subject,
		asset,
		hair_topology: hairTopologyBinding(artDirection, subject),
		marker_identity: markerIdentityBinding(artDirection, subject),
		decision,
		promotion_eligible: approved,
		reviewer: 'fixture-reviewer',
		reviewed_at: '2026-07-12T00:01:00.000Z',
		notes: approved ? 'Synthetic approval.' : 'Synthetic non-approval.',
		checklist: Object.fromEntries(
			REQUIRED_PAIR_REVIEW_CHECKS.map((check) => [check, approved]),
		),
		source_template_path: join(root, 'synthetic-review-template.json'),
	};
	const decisionRecord = {
		...decisionBase,
		review_id: reviewId(decisionBase),
	};
	await writeJson(decisionPath, decisionRecord);
	const decisionBytes = await readFile(decisionPath);
	await writeJson(pointerPath, {
		schema_version: 1,
		candidate_sha256: candidateSha256,
		decision,
		promotion_eligible: approved,
		record_path: decisionPath,
		record_sha256: hash(decisionBytes),
	});
	await writeJson(templatePath, {
		template_type: 'notebook-person-art-human-review',
		candidate_receipt_path: receiptPath,
	});
	await writeJson(packetPath, {
		schema_version: 1,
		packet_id: 'fixture-review-packet',
		candidate_count: 1,
		templates: ['pair-review.json'],
	});
	const packetBytes = await readFile(packetPath);
	const castBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-whole-cast-human-review-decision',
		source_packets: [{ path: packetPath, sha256: hash(packetBytes) }],
		cast: {
			named_count: 1,
			total_count: 1,
			members: [
				{
					subject_key: `npc:${npcId}`,
					subject,
					candidate_receipt_path: receiptPath,
					candidate_receipt_sha256: hash(receiptBytes),
					candidate_sha256: candidateSha256,
					raw_sha256: files.providerRaw.sha256,
					portrait_sha256: files.portrait.sha256,
					marker_sha256: files.marker.sha256,
					hair_topology: hairTopologyBinding(artDirection, subject),
					marker_identity: markerIdentityBinding(artDirection, subject),
				},
			],
		},
		decision: 'approved',
		promotion_eligible: true,
		reviewer: 'fixture-cast-reviewer',
		reviewed_at: '2026-07-12T00:02:00.000Z',
		notes: 'Synthetic whole-cast fixture approval.',
		checklist: {
			cast_distinctive: true,
			cast_hair_topology_distinctive: true,
		},
		source_template_path: join(root, 'whole-cast-template.json'),
	};
	await writeJson(castReviewPath, {
		...castBase,
		review_id: reviewId(castBase),
	});

	return {
		root,
		receiptPath,
		decisionPath,
		pointerPath,
		packetPath,
		castReviewPath,
		artDirectionPath,
		files,
		output: join(root, 'approved', 'v1'),
	};
}

async function promoteFixture(
	fixtureValue,
	output = fixtureValue.output,
	overrides = {},
) {
	return promoteNotebookPersonArt({
		repoRoot: fixtureValue.root,
		receiptPaths: [fixtureValue.receiptPath],
		decisionPaths: [fixtureValue.decisionPath],
		outputDir: output,
		inputsPath: fixtureValue.files.inputs.path,
		artDirectionPath: fixtureValue.artDirectionPath,
		mode: 'fixture',
		castReviewPath: fixtureValue.castReviewPath,
		...overrides,
	});
}

test('fixture promotion copies exact immutable inputs and emits a deterministic manifest', async () => {
	const value = await fixture();
	const first = await promoteFixture(
		value,
		join(value.root, 'release-a', 'v1'),
	);
	const second = await promoteFixture(
		value,
		join(value.root, 'release-b', 'v1'),
	);
	assert.equal(first.manifest.release_id, second.manifest.release_id);
	assert.equal(first.manifest.mode, 'fixture');
	assert.equal(first.manifest.entry_count, 1);
	assert.equal(
		first.manifest.entries[0].provider.request_id,
		'fixture-request-id',
	);
	assert.equal(first.manifest.entries[0].approval.decision, 'approved');
	assert.match(
		first.manifest.entries[0].approval.marker_identity_sha256,
		/^[a-f0-9]{64}$/,
	);
	assert.equal(
		first.manifest.entries[0].art.portrait.sha256,
		value.files.portrait.sha256,
	);
	assert.equal(
		first.manifest.entries[0].art.marker.sha256,
		value.files.marker.sha256,
	);

	const personRoot = join(value.root, 'release-a', 'v1', 'people', '7');
	for (const [approvedName, sourcePath] of [
		['portrait.png', value.files.portrait.path],
		['marker.png', value.files.marker.path],
		['prompt.txt', value.files.prompt.path],
		['input-record.json', value.files.inputRecord.path],
		['candidate-receipt.json', value.receiptPath],
		['review-decision.json', value.decisionPath],
		['provider-raw.png', value.files.providerRaw.path],
		['portrait-raw.png', value.files.portraitRaw.path],
		['marker-raw.png', value.files.markerRaw.path],
	]) {
		assert.deepEqual(
			await readFile(join(personRoot, approvedName)),
			await readFile(sourcePath),
		);
	}
});

test('promotes complete review packets without hand-assembling receipt pairs', async () => {
	const value = await fixture();
	const result = await promoteNotebookPersonArt({
		repoRoot: value.root,
		packetPaths: [value.packetPath],
		castReviewPath: value.castReviewPath,
		inputsPath: value.files.inputs.path,
		artDirectionPath: value.artDirectionPath,
		outputDir: value.output,
		mode: 'fixture',
	});

	assert.equal(result.manifest.entry_count, 1);
	assert.equal(result.manifest.entries[0].approval.decision, 'approved');
});

test.each(['pending', 'rejected'])(
	'rejects a %s review decision',
	async (decision) => {
		const value = await fixture({ decision });
		await assert.rejects(promoteFixture(value), /not promotion eligible/);
	},
);

test.each([
	['missing', (checklist) => delete checklist.production_quality],
	[
		'false',
		(checklist) => {
			checklist.production_quality = false;
		},
	],
])(
	'rejects an approved decision with a %s required pair checklist key',
	async (_state, mutate) => {
		const value = await fixture();
		const decision = JSON.parse(await readFile(value.decisionPath, 'utf8'));
		mutate(decision.checklist);
		const { review_id: _reviewId, ...base } = decision;
		decision.review_id = reviewId(base);
		await writeJson(value.decisionPath, decision);
		const decisionBytes = await readFile(value.decisionPath);
		const pointer = JSON.parse(await readFile(value.pointerPath, 'utf8'));
		pointer.record_sha256 = hash(decisionBytes);
		await writeJson(value.pointerPath, pointer);

		await assert.rejects(promoteFixture(value), /Pair approval checklist/);
	},
);

test('promotes pair approval plus an independent whole-cast approval', async () => {
	const value = await fixture();
	const decision = JSON.parse(await readFile(value.decisionPath, 'utf8'));
	assert.equal(decision.checklist.cast_distinctive, undefined);
	const cast = JSON.parse(await readFile(value.castReviewPath, 'utf8'));
	assert.equal(cast.checklist.cast_distinctive, true);

	const result = await promoteFixture(value);
	assert.equal(result.manifest.entries[0].approval.decision, 'approved');
	assert.equal(
		result.manifest.approval.whole_cast_visual_review.review_id,
		cast.review_id,
	);
});

test('promotion rejects mutated or mismatched subject identity bindings', async () => {
	const mutated = await fixture();
	const sidecar = JSON.parse(await readFile(mutated.artDirectionPath, 'utf8'));
	sidecar.npcs[0].portrait_identity.hair_topology.rear.family = 'mutated-rear';
	await writeJson(mutated.artDirectionPath, sidecar);
	await assert.rejects(
		promoteFixture(mutated),
		/Pair review decision hair topology does not match current schema-v4 hair topology/,
	);

	const markerMutated = await fixture();
	const markerSidecar = JSON.parse(
		await readFile(markerMutated.artDirectionPath, 'utf8'),
	);
	markerSidecar.npcs[0].marker_identity.empty_hand_pose =
		'substituted-hands-behind-back';
	await writeJson(markerMutated.artDirectionPath, markerSidecar);
	await assert.rejects(
		promoteFixture(markerMutated),
		/Pair review decision marker identity does not match current schema-v4 marker identity/,
	);

	const mismatched = await fixture();
	const cast = JSON.parse(await readFile(mismatched.castReviewPath, 'utf8'));
	cast.cast.members[0].hair_topology = {
		...cast.cast.members[0].hair_topology,
		subject_key: 'npc:999',
	};
	const { review_id: _reviewId, ...castBase } = cast;
	cast.review_id = reviewId(castBase);
	await writeJson(mismatched.castReviewPath, cast);
	await assert.rejects(
		promoteFixture(mismatched),
		/Whole-cast member npc:7 does not match the approved candidate/,
	);

	const markerMismatched = await fixture();
	const markerCast = JSON.parse(
		await readFile(markerMismatched.castReviewPath, 'utf8'),
	);
	markerCast.cast.members[0].marker_identity = {
		...markerCast.cast.members[0].marker_identity,
		subject_key: 'npc:999',
	};
	const { review_id: _markerReviewId, ...markerCastBase } = markerCast;
	markerCast.review_id = reviewId(markerCastBase);
	await writeJson(markerMismatched.castReviewPath, markerCast);
	await assert.rejects(
		promoteFixture(markerMismatched),
		/Whole-cast member npc:7 does not match the approved candidate/,
	);
});

test('unrelated topology changes do not invalidate promotion', async () => {
	const value = await fixture();
	const sidecar = JSON.parse(await readFile(value.artDirectionPath, 'utf8'));
	sidecar.npcs[1].portrait_identity.hair_topology.silhouette.family =
		'unrelated-silhouette-v2';
	sidecar.npcs[1].marker_identity.stance = 'unrelated stance v2';
	await writeJson(value.artDirectionPath, sidecar);

	const result = await promoteFixture(value);
	assert.equal(result.manifest.entry_count, 1);
});

test('rejects singleton review packets without a whole-cast decision', async () => {
	const value = await fixture();
	await assert.rejects(
		promoteFixture(value, value.output, { castReviewPath: undefined }),
		/requires one immutable --cast-review decision/,
	);
});

test('rejects invented pair and whole-cast review ids', async () => {
	const pair = await fixture();
	const pairDecision = JSON.parse(await readFile(pair.decisionPath, 'utf8'));
	pairDecision.review_id = '0'.repeat(24);
	await writeJson(pair.decisionPath, pairDecision);
	const pointer = JSON.parse(await readFile(pair.pointerPath, 'utf8'));
	pointer.record_sha256 = hash(await readFile(pair.decisionPath));
	await writeJson(pair.pointerPath, pointer);
	await assert.rejects(promoteFixture(pair), /id does not match its content/);

	const cast = await fixture();
	const castDecision = JSON.parse(await readFile(cast.castReviewPath, 'utf8'));
	castDecision.review_id = 'f'.repeat(24);
	await writeJson(cast.castReviewPath, castDecision);
	await assert.rejects(
		promoteFixture(cast),
		/Whole-cast review id does not match its content/,
	);
});

test.each([
	['receipt', (value) => value.receiptPath, Buffer.from('\n')],
	['decision', (value) => value.decisionPath, Buffer.from('\n')],
	['prompt', (value) => value.files.prompt.path, Buffer.from('tampered')],
	['config', (value) => value.files.config.path, Buffer.from(' ')],
	['reference', (value) => value.files.reference.path, Buffer.from('tampered')],
	[
		'provider raw',
		(value) => value.files.providerRaw.path,
		Buffer.from('tampered'),
	],
	[
		'portrait raw',
		(value) => value.files.portraitRaw.path,
		Buffer.from('tampered'),
	],
	[
		'marker raw',
		(value) => value.files.markerRaw.path,
		Buffer.from('tampered'),
	],
	['portrait', (value) => value.files.portrait.path, Buffer.from('tampered')],
	['marker', (value) => value.files.marker.path, Buffer.from('tampered')],
])('rejects %s hash tampering', async (_label, pathFor, suffix) => {
	const value = await fixture();
	const path = pathFor(value);
	await writeFile(path, Buffer.concat([await readFile(path), suffix]));
	await assert.rejects(promoteFixture(value), /hash does not match/);
});

test('rejects semantic input-record tampering', async () => {
	const value = await fixture();
	const record = JSON.parse(
		await readFile(value.files.inputRecord.path, 'utf8'),
	);
	record.name = 'Changed after generation';
	await writeJson(value.files.inputRecord.path, record);
	await assert.rejects(
		promoteFixture(value),
		/Input record hash does not match/,
	);
});

test('allows an unrelated current catalog change and binds its hash into the release', async () => {
	const value = await fixture();
	const currentInputs = JSON.parse(
		await readFile(value.files.inputs.path, 'utf8'),
	);
	currentInputs.npcs.push({
		npc_id: 8,
		name: 'Changed after this candidate job',
		pair_prompt: 'New subject pair.',
		portrait_prompt: 'New subject portrait.',
		marker_prompt: 'New subject marker.',
	});
	await writeJson(value.files.inputs.path, currentInputs);

	const promoted = await promoteFixture(value);
	const currentInputsBytes = await readFile(value.files.inputs.path);
	assert.equal(
		promoted.manifest.provenance.npc_art_inputs.sha256,
		hash(currentInputsBytes),
	);
	assert.deepEqual(
		await readFile(join(value.output, 'npc-art-inputs.json')),
		currentInputsBytes,
	);
});

test('allows an unavailable historical catalog when the explicit current record matches', async () => {
	const value = await fixture();
	const currentInputsPath = join(value.root, 'current', 'npc-art-inputs.json');
	const receiptBytes = await readFile(value.receiptPath);
	await write(currentInputsPath, await readFile(value.files.inputs.path));
	await rm(value.files.inputs.path);

	const promoted = await promoteFixture(value, value.output, {
		inputsPath: currentInputsPath,
	});
	assert.equal(
		promoted.manifest.entries[0].generation.receipt_sha256,
		hash(receiptBytes),
	);
	assert.deepEqual(
		await readFile(join(value.output, 'people', '7', 'candidate-receipt.json')),
		receiptBytes,
	);
});

test('rejects a candidate whose current catalog record changed', async () => {
	const value = await fixture();
	const currentInputs = JSON.parse(
		await readFile(value.files.inputs.path, 'utf8'),
	);
	currentInputs.npcs[0].name = 'Changed after generation';
	await writeJson(value.files.inputs.path, currentInputs);

	await assert.rejects(
		promoteFixture(value),
		/Input record and current catalog entry does not match/,
	);
});

test('rejects duplicate subjects and candidate pairs', async () => {
	const value = await fixture();
	await assert.rejects(
		promoteNotebookPersonArt({
			repoRoot: value.root,
			receiptPaths: [value.receiptPath, value.receiptPath],
			decisionPaths: [value.decisionPath, value.decisionPath],
			outputDir: value.output,
			inputsPath: value.files.inputs.path,
			artDirectionPath: value.artDirectionPath,
			mode: 'fixture',
		}),
		/Duplicate approved subject/,
	);
});

test('production mode rejects a release missing 23 numeric NPC ids and fallback', async () => {
	const value = await fixture();
	await write(
		join(
			value.root,
			'parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json',
		),
		await readFile(value.artDirectionPath),
	);
	await assert.rejects(
		promoteFixture(value, value.output, {
			mode: 'production',
			artDirectionPath: undefined,
		}),
		/requires 23 numeric NPC ids plus one fallback/,
	);
});

test('production mode refuses a substitute art-direction sidecar', async () => {
	const value = await fixture();
	await assert.rejects(
		promoteFixture(value, value.output, { mode: 'production' }),
		/requires the canonical schema-v4 NPC art direction sidecar/,
	);
});

test('help documents production and unpaid fixture modes', () => {
	assert.match(helpText(), /--inputs <path>/);
	assert.match(helpText(), /23 numeric NPC ids plus fallback/);
	assert.match(
		helpText(),
		/fixture permits a partial synthetic set for unpaid tests/,
	);
});
