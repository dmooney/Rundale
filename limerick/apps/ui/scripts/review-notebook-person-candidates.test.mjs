// @vitest-environment node

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, test } from 'vitest';
import { PNG } from 'pngjs';

import {
	prepareReviewPacket,
	prepareWholeCastReview,
	readReviewStatus,
	submitReviewDecision,
	submitWholeCastReviewDecision,
} from './review-notebook-person-candidates.mjs';

const cleanupPaths = [];

afterEach(async () => {
	await Promise.all(
		cleanupPaths
			.splice(0)
			.map((path) => rm(path, { recursive: true, force: true })),
	);
});

function pngFixture({ transparent = false } = {}) {
	const png = new PNG({ width: 64, height: 64 });
	for (let y = 0; y < png.height; y += 1) {
		for (let x = 0; x < png.width; x += 1) {
			const offset = (y * png.width + x) * 4;
			const subject = x >= 22 && x <= 41 && y >= 12 && y <= 52;
			if (subject) {
				const value = 40 + ((x + y) % 6) * 20;
				png.data[offset] = value;
				png.data[offset + 1] = value;
				png.data[offset + 2] = value;
				png.data[offset + 3] = 255;
			} else {
				png.data[offset] = transparent ? 0 : 255;
				png.data[offset + 1] = 0;
				png.data[offset + 2] = transparent ? 0 : 255;
				png.data[offset + 3] = transparent ? 0 : 255;
			}
		}
	}
	return PNG.sync.write(png, { colorType: 6 });
}

function hash(bytes) {
	return createHash('sha256').update(bytes).digest('hex');
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
		silhouette: `${seed} compact silhouette; no object/scenery`,
		stance: `${seed} grounded stance with empty hands; no object/scenery`,
		empty_hand_pose: `${seed}-hands-at-sides`,
		readability_cues: [
			{ kind: 'body-shape', description: `${seed} body shape` },
			{ kind: 'hair-or-headwear', description: `${seed} hair shape` },
			{ kind: 'clothing', description: `${seed} clothing shape` },
		],
		tiny_readability_notes: [`Keep ${seed} readable before facial detail.`],
	};
}

async function writeArtDirection(path) {
	await writeFile(
		path,
		`${JSON.stringify(
			{
				schema_version: 4,
				npcs: [
					{
						npc_id: 4,
						portrait_identity: { hair_topology: hairTopology('roisin') },
						marker_identity: markerIdentity('roisin'),
					},
					{
						npc_id: 99,
						portrait_identity: { hair_topology: hairTopology('unrelated') },
						marker_identity: markerIdentity('unrelated'),
					},
				],
				fallback: {
					portrait_identity: { hair_topology: hairTopology('fallback') },
					marker_identity: markerIdentity('fallback'),
				},
			},
			null,
			'\t',
		)}\n`,
	);
}

function pairPngFixture() {
	const left = PNG.sync.read(pngFixture());
	const right = PNG.sync.read(pngFixture());
	const sheet = new PNG({ width: 128, height: 64 });
	for (let row = 0; row < 64; row += 1) {
		left.data.copy(sheet.data, row * 128 * 4, row * 64 * 4, (row + 1) * 64 * 4);
		right.data.copy(
			sheet.data,
			(row * 128 + 64) * 4,
			row * 64 * 4,
			(row + 1) * 64 * 4,
		);
	}
	return PNG.sync.write(sheet, { colorType: 6 });
}

async function candidateFixture(assetKind = 'portrait') {
	const root = await mkdtemp(join(tmpdir(), 'rundale-art-review-'));
	cleanupPaths.push(root);
	const objectRoot = join(root, 'objects', 'ab', 'candidate-job');
	const attemptRoot = join(objectRoot, 'attempts', 'attempt-1');
	const receiptPath = join(objectRoot, 'receipt.json');
	const rawPath = join(attemptRoot, 'raw.png');
	const candidatePath = join(attemptRoot, 'candidate.png');
	const raw = pngFixture();
	const candidate = pngFixture({ transparent: true });
	await import('node:fs/promises').then(({ mkdir }) =>
		mkdir(attemptRoot, { recursive: true }),
	);
	await Promise.all([
		writeFile(rawPath, raw),
		writeFile(candidatePath, candidate),
	]);
	const receipt = {
		schema_version: 1,
		receipt_type: 'notebook-person-art-candidate',
		job_id: 'candidate-job',
		run_id: 'test-run',
		status: 'candidate',
		review: { status: 'pending', reviewer: null, reviewed_at: null },
		promotion: { eligible: false, reason: 'Human review required' },
		subject: {
			kind: 'npc',
			npc_id: 4,
			name: 'Roisin Connolly',
			input_record_sha256: 'input-record-hash',
		},
		asset: { kind: assetKind, candidate_index: 1 },
		provider: {
			model: 'gpt-image-2-test',
			request_id: 'req_review_test',
		},
		provenance: { prompt_sha256: 'prompt-hash' },
		artifact: {
			raw_path: rawPath,
			raw_sha256: hash(raw),
			candidate_path: candidatePath,
			candidate_sha256: hash(candidate),
			media_type: 'image/png',
			width: 64,
			height: 64,
		},
	};
	await writeFile(receiptPath, `${JSON.stringify(receipt, null, '\t')}\n`);
	return { root, objectRoot, receiptPath, rawPath, candidatePath };
}

async function pairCandidateFixture({ fallback = false } = {}) {
	const root = await mkdtemp(join(tmpdir(), 'rundale-art-pair-review-'));
	cleanupPaths.push(root);
	const objectRoot = join(root, 'objects', 'cd', 'pair-job');
	const attemptRoot = join(objectRoot, 'attempts', 'attempt-1');
	const receiptPath = join(objectRoot, 'receipt.json');
	const rawPath = join(attemptRoot, 'raw.png');
	const artDirectionPath = join(root, 'npc-art-direction-v1.json');
	const raw = pairPngFixture();
	await import('node:fs/promises').then(({ mkdir }) =>
		mkdir(attemptRoot, { recursive: true }),
	);
	await Promise.all([
		writeFile(rawPath, raw),
		writeArtDirection(artDirectionPath),
	]);
	const children = {};
	for (const kind of ['portrait', 'marker']) {
		const childRaw = pngFixture();
		const candidate = pngFixture({ transparent: true });
		const childRawPath = join(attemptRoot, `${kind}-raw.png`);
		const candidatePath = join(attemptRoot, `${kind}-candidate.png`);
		await Promise.all([
			writeFile(childRawPath, childRaw),
			writeFile(candidatePath, candidate),
		]);
		children[kind] = {
			raw_path: childRawPath,
			raw_sha256: hash(childRaw),
			candidate_path: candidatePath,
			candidate_sha256: hash(candidate),
			media_type: 'image/png',
			width: 64,
			height: 64,
		};
	}
	const receipt = {
		schema_version: 1,
		receipt_type: 'notebook-person-art-pair-candidate',
		job_id: 'pair-job',
		run_id: 'pair-test-run',
		status: 'candidate',
		review: { status: 'pending', reviewer: null, reviewed_at: null },
		promotion: { eligible: false, reason: 'Atomic human review required' },
		identity_lock: {
			generation: 'single-provider-request',
			status: 'pending-human-review',
		},
		subject: {
			kind: fallback ? 'fallback' : 'npc',
			npc_id: fallback ? null : 4,
			name: fallback ? 'Unknown parish neighbour' : 'Roisin Connolly',
			input_record_sha256: 'input-record-hash',
		},
		asset: {
			kind: 'pair',
			candidate_index: 1,
			children: ['portrait', 'marker'],
		},
		provider: {
			model: 'gpt-image-2-test',
			request_id: 'req_pair_review_test',
		},
		provenance: { prompt_sha256: 'pair-prompt-hash' },
		artifact: {
			raw_path: rawPath,
			raw_sha256: hash(raw),
			media_type: 'image/png',
			width: 128,
			height: 64,
			children,
		},
	};
	await writeFile(receiptPath, `${JSON.stringify(receipt, null, '\t')}\n`);
	return {
		root,
		objectRoot,
		receiptPath,
		rawPath,
		children,
		artDirectionPath,
	};
}

async function prepare(fixture, packet = 'packet') {
	const result = await prepareReviewPacket({
		receiptPaths: [fixture.receiptPath],
		outputDir: join(fixture.root, packet),
		packetId: packet,
		artDirectionPath: fixture.artDirectionPath,
		fixture: true,
	});
	return { ...result, artDirectionPath: fixture.artDirectionPath };
}

async function completeTemplate(templatePath, decision, overrides = {}) {
	const template = JSON.parse(await readFile(templatePath, 'utf8'));
	template.decision = decision;
	template.reviewer = 'human-reviewer';
	template.notes =
		decision === 'rejected' ? 'Does not meet production bar.' : '';
	for (const key of Object.keys(template.checklist))
		template.checklist[key] = true;
	Object.assign(template, overrides);
	await writeFile(templatePath, `${JSON.stringify(template, null, '\t')}\n`);
	return template;
}

test('prepare creates a self-contained visual packet and hash-bound template', async () => {
	const fixture = await candidateFixture();
	const packet = await prepare(fixture);
	const html = await readFile(packet.htmlPath, 'utf8');
	const template = JSON.parse(await readFile(packet.templatePaths[0], 'utf8'));
	assert.match(html, /data:image\/png;base64,/);
	assert.match(html, /Roisin Connolly/);
	assert.match(html, /99x112/);
	assert.equal(template.decision, null);
	assert.equal(template.reviewer, null);
	assert.equal(template.subject.npc_id, 4);
	assert.equal(template.candidate_sha256.length, 64);
	assert(Object.values(template.checklist).every((answer) => answer === null));
});

test('paired review is atomic and requires an explicit cross-asset identity check', async () => {
	const fixture = await pairCandidateFixture();
	const packet = await prepare(fixture, 'pair-packet');
	const html = await readFile(packet.htmlPath, 'utf8');
	const template = JSON.parse(await readFile(packet.templatePaths[0], 'utf8'));
	assert.match(html, /identity-locked portrait \+ marker/);
	assert.match(html, /Preserved 2-cell provider response/);
	assert.match(html, /Portrait 99x112/);
	assert.match(html, /Marker 60x85/);
	assert.equal(template.asset.kind, 'pair');
	assert.equal(template.hair_topology.schema_version, 4);
	assert.equal(template.hair_topology.subject_key, 'npc:4');
	assert.match(template.hair_topology.sha256, /^[a-f0-9]{64}$/);
	assert.equal(template.marker_identity.subject_key, 'npc:4');
	assert.equal(
		template.marker_identity.marker_identity.empty_hand_pose,
		'roisin-hands-at-sides',
	);
	assert.match(template.marker_identity.sha256, /^[a-f0-9]{64}$/);
	assert.match(html, /Expected marker identity/);
	assert.match(html, /roisin compact silhouette/);
	assert.equal(template.checklist.cross_asset_identity, null);
	assert.equal(template.checklist.cast_distinctive, undefined);
	assert.equal(template.checklist.cast_hair_topology_distinctive, undefined);
	assert.equal(template.checklist.hair_front_matches, null);
	assert.equal(template.checklist.hair_rear_matches, null);
	assert.equal(template.checklist.hair_covering_matches, null);
	assert.equal(template.checklist.hair_silhouette_matches, null);
	assert.equal(template.checklist.correct_surface_split, null);
	assert.equal(template.checklist.atomic_rerender_understood, null);

	await completeTemplate(packet.templatePaths[0], 'approved');
	const decision = await submitReviewDecision({
		templatePath: packet.templatePaths[0],
		artDirectionPath: fixture.artDirectionPath,
		fixture: true,
	});
	assert.equal(decision.record.asset.kind, 'pair');
	assert.equal(decision.record.promotion_eligible, true);
	assert.equal(
		decision.record.marker_identity.sha256,
		template.marker_identity.sha256,
	);
	const status = await readReviewStatus({ receiptPath: fixture.receiptPath });
	assert.equal(status.status, 'approved');
});

test('non-canonical review sidecars require explicit fixture mode', async () => {
	const fixture = await pairCandidateFixture();
	await assert.rejects(
		prepareReviewPacket({
			receiptPaths: [fixture.receiptPath],
			outputDir: join(fixture.root, 'non-canonical-sidecar'),
			packetId: 'non-canonical-sidecar',
			artDirectionPath: fixture.artDirectionPath,
		}),
		/require explicit fixture mode/,
	);
});

test('approval requires all checks and writes an immutable hash-linked record', async () => {
	const fixture = await candidateFixture();
	const packet = await prepare(fixture);
	await completeTemplate(packet.templatePaths[0], 'approved');
	const decision = await submitReviewDecision({
		templatePath: packet.templatePaths[0],
	});
	assert.equal(decision.record.decision, 'approved');
	assert.equal(decision.record.promotion_eligible, true);
	const status = await readReviewStatus({ receiptPath: fixture.receiptPath });
	assert.equal(status.status, 'approved');
	assert.equal(status.promotion_eligible, true);
	await assert.rejects(
		submitReviewDecision({ templatePath: packet.templatePaths[0] }),
		/already has a review pointer/,
	);
});

test('only one immutable whole-cast decision can assert cast distinctiveness', async () => {
	const named = await pairCandidateFixture();
	const fallback = await pairCandidateFixture({ fallback: true });
	const packet = await prepareReviewPacket({
		repoRoot: named.root,
		receiptPaths: [named.receiptPath, fallback.receiptPath],
		outputDir: join(named.root, 'cast-source-packet'),
		packetId: 'cast-source-packet',
		artDirectionPath: named.artDirectionPath,
		fixture: true,
	});
	const pairTemplate = JSON.parse(
		await readFile(packet.templatePaths[0], 'utf8'),
	);
	assert.equal(pairTemplate.checklist.cast_distinctive, undefined);
	assert.equal(
		pairTemplate.checklist.cast_hair_topology_distinctive,
		undefined,
	);

	await assert.rejects(
		prepareWholeCastReview({
			repoRoot: named.root,
			packetPaths: [packet.manifestPath],
			expectedNamedCount: 2,
			outputDir: join(named.root, 'incomplete-cast'),
			artDirectionPath: named.artDirectionPath,
			fixture: true,
		}),
		/requires exactly 2 named candidates plus fallback/,
	);

	const cast = await prepareWholeCastReview({
		repoRoot: named.root,
		packetPaths: [packet.manifestPath],
		expectedNamedCount: 1,
		outputDir: join(named.root, 'whole-cast'),
		artDirectionPath: named.artDirectionPath,
		fixture: true,
	});
	const template = JSON.parse(await readFile(cast.templatePath, 'utf8'));
	assert.equal(template.cast.named_count, 1);
	assert.equal(template.cast.total_count, 2);
	assert.equal(template.cast.members[0].hair_topology.subject_key, 'npc:4');
	assert.equal(template.cast.members[1].hair_topology.subject_key, 'fallback');
	assert.equal(template.cast.members[0].marker_identity.subject_key, 'npc:4');
	assert.equal(
		template.cast.members[1].marker_identity.subject_key,
		'fallback',
	);
	assert.match(
		await readFile(cast.htmlPath, 'utf8'),
		/Expected marker identity/,
	);
	assert.equal(template.checklist.cast_distinctive, null);
	assert.equal(template.checklist.cast_hair_topology_distinctive, null);
	template.decision = 'approved';
	template.reviewer = 'whole-cast-reviewer';
	template.checklist.cast_distinctive = true;
	template.checklist.cast_hair_topology_distinctive = true;
	await writeFile(
		cast.templatePath,
		`${JSON.stringify(template, null, '\t')}\n`,
	);
	const decision = await submitWholeCastReviewDecision({
		repoRoot: named.root,
		templatePath: cast.templatePath,
		expectedNamedCount: 1,
		artDirectionPath: named.artDirectionPath,
		fixture: true,
	});
	assert.equal(decision.record.promotion_eligible, true);
	assert.equal(decision.record.cast.members.length, 2);
	assert.match(decision.record.review_id, /^[a-f0-9]{24}$/);
});

test('rejection requires a failed check and notes, and never enables promotion', async () => {
	const fixture = await candidateFixture('marker');
	const packet = await prepare(fixture);
	const template = await completeTemplate(packet.templatePaths[0], 'rejected');
	template.checklist.production_quality = false;
	await writeFile(
		packet.templatePaths[0],
		`${JSON.stringify(template, null, '\t')}\n`,
	);
	const decision = await submitReviewDecision({
		templatePath: packet.templatePaths[0],
	});
	assert.equal(decision.record.decision, 'rejected');
	assert.equal(decision.record.promotion_eligible, false);
});

test('decision detects candidate tampering after packet preparation', async () => {
	const fixture = await candidateFixture();
	const packet = await prepare(fixture);
	await completeTemplate(packet.templatePaths[0], 'approved');
	await writeFile(
		fixture.candidatePath,
		Buffer.concat([
			await readFile(fixture.candidatePath),
			Buffer.from('tamper'),
		]),
	);
	await assert.rejects(
		submitReviewDecision({ templatePath: packet.templatePaths[0] }),
		/Candidate artifact hash does not match/,
	);
});

test('pair decision rejects topology mutation after review preparation', async () => {
	const fixture = await pairCandidateFixture();
	const packet = await prepare(fixture, 'topology-tamper');
	await completeTemplate(packet.templatePaths[0], 'approved');
	const sidecar = JSON.parse(await readFile(fixture.artDirectionPath, 'utf8'));
	sidecar.npcs[0].portrait_identity.hair_topology.front.family =
		'mutated-front';
	await writeFile(
		fixture.artDirectionPath,
		`${JSON.stringify(sidecar, null, '\t')}\n`,
	);

	await assert.rejects(
		submitReviewDecision({
			templatePath: packet.templatePaths[0],
			artDirectionPath: fixture.artDirectionPath,
			fixture: true,
		}),
		/does not match current schema-v4 hair topology/,
	);
});

test('pair decision rejects marker identity mutation after review preparation', async () => {
	const fixture = await pairCandidateFixture();
	const packet = await prepare(fixture, 'marker-identity-tamper');
	await completeTemplate(packet.templatePaths[0], 'approved');
	const sidecar = JSON.parse(await readFile(fixture.artDirectionPath, 'utf8'));
	sidecar.npcs[0].marker_identity.stance = 'substituted marker stance';
	await writeFile(
		fixture.artDirectionPath,
		`${JSON.stringify(sidecar, null, '\t')}\n`,
	);

	await assert.rejects(
		submitReviewDecision({
			templatePath: packet.templatePaths[0],
			artDirectionPath: fixture.artDirectionPath,
			fixture: true,
		}),
		/does not match current schema-v4 marker identity/,
	);
});

test('whole-cast decision rejects marker identity substitution after preparation', async () => {
	const named = await pairCandidateFixture();
	const fallback = await pairCandidateFixture({ fallback: true });
	const packet = await prepareReviewPacket({
		repoRoot: named.root,
		receiptPaths: [named.receiptPath, fallback.receiptPath],
		outputDir: join(named.root, 'marker-cast-source'),
		packetId: 'marker-cast-source',
		artDirectionPath: named.artDirectionPath,
		fixture: true,
	});
	const cast = await prepareWholeCastReview({
		repoRoot: named.root,
		packetPaths: [packet.manifestPath],
		expectedNamedCount: 1,
		outputDir: join(named.root, 'marker-cast-review'),
		artDirectionPath: named.artDirectionPath,
		fixture: true,
	});
	const template = JSON.parse(await readFile(cast.templatePath, 'utf8'));
	template.decision = 'approved';
	template.reviewer = 'whole-cast-reviewer';
	template.checklist.cast_distinctive = true;
	template.checklist.cast_hair_topology_distinctive = true;
	await writeFile(
		cast.templatePath,
		`${JSON.stringify(template, null, '\t')}\n`,
	);
	const sidecar = JSON.parse(await readFile(named.artDirectionPath, 'utf8'));
	sidecar.fallback.marker_identity.silhouette =
		'substituted fallback silhouette';
	await writeFile(
		named.artDirectionPath,
		`${JSON.stringify(sidecar, null, '\t')}\n`,
	);

	await assert.rejects(
		submitWholeCastReviewDecision({
			repoRoot: named.root,
			templatePath: cast.templatePath,
			expectedNamedCount: 1,
			artDirectionPath: named.artDirectionPath,
			fixture: true,
		}),
		/does not match current schema-v4 marker identity/,
	);
});

test('unrelated sidecar record changes do not invalidate a pair review', async () => {
	const fixture = await pairCandidateFixture();
	const packet = await prepare(fixture, 'unrelated-topology-change');
	await completeTemplate(packet.templatePaths[0], 'approved');
	const sidecar = JSON.parse(await readFile(fixture.artDirectionPath, 'utf8'));
	sidecar.npcs[1].portrait_identity.hair_topology.front.family =
		'unrelated-front-v2';
	sidecar.npcs[1].marker_identity.stance = 'unrelated stance v2';
	await writeFile(
		fixture.artDirectionPath,
		`${JSON.stringify(sidecar, null, '\t')}\n`,
	);

	const decision = await submitReviewDecision({
		templatePath: packet.templatePaths[0],
		artDirectionPath: fixture.artDirectionPath,
		fixture: true,
	});
	assert.equal(
		decision.record.hair_topology.sha256,
		JSON.parse(await readFile(packet.templatePaths[0], 'utf8')).hair_topology
			.sha256,
	);
	assert.match(decision.record.marker_identity.sha256, /^[a-f0-9]{64}$/);
});

test('status detects a modified immutable decision record', async () => {
	const fixture = await candidateFixture();
	const packet = await prepare(fixture);
	await completeTemplate(packet.templatePaths[0], 'approved');
	const decision = await submitReviewDecision({
		templatePath: packet.templatePaths[0],
	});
	await writeFile(
		decision.recordPath,
		`${await readFile(decision.recordPath, 'utf8')}\n`,
	);
	await assert.rejects(
		readReviewStatus({ receiptPath: fixture.receiptPath }),
		/Review decision hash does not match/,
	);
});
