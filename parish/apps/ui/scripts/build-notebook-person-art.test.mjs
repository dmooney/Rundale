import { createHash } from 'node:crypto';
import {
	mkdtemp,
	mkdir,
	readFile,
	readdir,
	rm,
	writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { afterEach, describe, expect, test } from 'vitest';
import { buildNotebookPersonArt } from './build-notebook-person-art.mjs';
import { verifyNotebookPersonArtFreshness } from './verify-notebook-person-art-freshness.mjs';
import {
	computeReviewId,
	hairTopologyBinding,
	markerIdentityBinding,
	pairCandidateDigest,
	REQUIRED_PAIR_REVIEW_CHECKS,
	subjectKey,
} from './notebook-person-art-approval-contract.mjs';
import {
	createImage,
	decodePngBytes,
	encodePng,
	getPixel,
	setPixel,
} from './notebook-person-art-png.mjs';

const temporaryRoots = [];

function hash(bytes) {
	return createHash('sha256').update(bytes).digest('hex');
}

function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
	return `{${Object.entries(value)
		.toSorted(([left], [right]) => left.localeCompare(right))
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

function topologyBinding(subject) {
	const seed = subject.npc_id ?? 'fallback';
	const vector = {
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
	return hairTopologyBinding(
		{
			schema_version: 4,
			npcs:
				subject.kind === 'npc'
					? [
							{
								npc_id: subject.npc_id,
								portrait_identity: { hair_topology: vector },
							},
						]
					: [],
			fallback:
				subject.kind === 'fallback'
					? { portrait_identity: { hair_topology: vector } }
					: null,
		},
		subject,
	);
}

function markerBinding(subject) {
	const seed = subject.npc_id ?? 'fallback';
	const markerIdentity = {
		composition: 'character-only',
		silhouette: `${seed} silhouette`,
		stance: `${seed} stance`,
		empty_hand_pose: `${seed} empty-hand pose`,
		readability_cues: [
			{ kind: 'face', description: `${seed} facial geometry` },
			{ kind: 'stance', description: `${seed} posture` },
		],
		tiny_readability_notes: [`${seed} remains distinct at runtime size`],
	};
	return markerIdentityBinding(
		{
			schema_version: 4,
			npcs:
				subject.kind === 'npc'
					? [{ npc_id: subject.npc_id, marker_identity: markerIdentity }]
					: [],
			fallback:
				subject.kind === 'fallback'
					? { marker_identity: markerIdentity }
					: null,
		},
		subject,
	);
}

async function write(path, bytes) {
	await mkdir(dirname(path), { recursive: true });
	await writeFile(path, bytes);
}

function transparentArt(width, height, color, vertical = false) {
	const image = createImage(width, height);
	if (vertical) {
		for (let y = 0; y < height; y += 1)
			setPixel(image, Math.floor(width / 2), y, [...color, 255]);
	} else {
		for (let y = 1; y < height - 1; y += 1) {
			for (let x = 1; x < width - 1; x += 1)
				setPixel(image, x, y, [...color, 255]);
		}
	}
	return encodePng(image);
}

async function promotedSource(
	root,
	releaseRoot,
	releasePath,
	sourcePath,
	bytes,
) {
	await write(join(releaseRoot, releasePath), bytes);
	await write(join(root, sourcePath), bytes);
	return { path: releasePath, sha256: hash(bytes), source_path: sourcePath };
}

async function makeEntry(
	root,
	releaseRoot,
	id,
	name,
	portraitBytes,
	markerBytes,
	sourceBytes,
	shared,
) {
	const key = id === null ? 'fallback' : String(id);
	const releasePersonRoot = `people/${key}`;
	const sourceRoot = `candidate-source/${key}`;
	const promptBytes = Buffer.from(`fixture ${key}\n`);
	const inputRecord = id === null ? { key } : { npc_id: id, key };
	const inputRecordBytes = Buffer.from(`${JSON.stringify(inputRecord)}\n`);
	await write(
		join(releaseRoot, `${releasePersonRoot}/prompt.txt`),
		promptBytes,
	);
	await write(
		join(releaseRoot, `${releasePersonRoot}/input-record.json`),
		inputRecordBytes,
	);
	await write(join(root, `${sourceRoot}/provider-raw.png`), sourceBytes);
	await write(
		join(releaseRoot, `${releasePersonRoot}/provider-raw.png`),
		sourceBytes,
	);

	const art = {};
	for (const [kind, bytes] of [
		['portrait', portraitBytes],
		['marker', markerBytes],
	]) {
		const image = decodePngBytes(bytes);
		await write(join(releaseRoot, `${releasePersonRoot}/${kind}.png`), bytes);
		await write(join(root, `${sourceRoot}/${kind}-candidate.png`), bytes);
		await write(join(root, `${sourceRoot}/${kind}-raw.png`), bytes);
		await write(
			join(releaseRoot, `${releasePersonRoot}/${kind}-raw.png`),
			bytes,
		);
		art[kind] = {
			master_path: `${releasePersonRoot}/${kind}.png`,
			sha256: hash(bytes),
			raw_path: `${releasePersonRoot}/${kind}-raw.png`,
			media_type: 'image/png',
			width: image.width,
			height: image.height,
			source_candidate_path: `${sourceRoot}/${kind}-candidate.png`,
			source_raw_path: `${sourceRoot}/${kind}-raw.png`,
			source_raw_sha256: hash(bytes),
			validation: {},
		};
	}
	const subject = {
		kind: id === null ? 'fallback' : 'npc',
		npc_id: id,
		name,
		input_record_sha256: hash(canonicalJson(inputRecord)),
	};
	const asset = {
		kind: 'pair',
		candidate_index: 1,
		children: ['portrait', 'marker'],
	};
	const provider = {
		...shared.config.provider,
		request_id: `request-${key}`,
	};
	const promptSha256 = hash(promptBytes.subarray(0, -1));
	const identity = {
		schema_version: 1,
		pipeline_revision: shared.config.pipeline_revision,
		provider: {
			id: provider.id,
			adapter: provider.adapter,
			model: provider.model,
			request: provider.request,
		},
		raw_output: shared.config.raw_output,
		validation: shared.config.validation,
		reference_inputs: [
			{ id: shared.reference.id, sha256: shared.reference.sha256 },
		],
		subject_kind: subject.kind,
		npc_id: subject.npc_id,
		input_record_sha256: subject.input_record_sha256,
		asset_kind: 'pair',
		candidate_index: 1,
		prompt_sha256: promptSha256,
	};
	const jobId = hash(canonicalJson(identity));
	const receiptValue = {
		schema_version: 1,
		receipt_type: 'notebook-person-art-pair-candidate',
		job_id: jobId,
		status: 'candidate',
		review: { status: 'pending', reviewer: null, reviewed_at: null },
		promotion: { eligible: false, reason: 'Human review required' },
		subject,
		asset,
		provider,
		provenance: {
			config_path: shared.config.source_path,
			config_sha256: shared.config.sha256,
			inputs_path: shared.inputs.source_path,
			inputs_sha256: shared.inputs.sha256,
			prompt_path: `${sourceRoot}/prompt.txt`,
			prompt_sha256: promptSha256,
			input_record_path: `${sourceRoot}/input-record.json`,
			reference_inputs: [
				{
					id: shared.reference.id,
					path: shared.reference.source_path,
					purpose: shared.reference.purpose,
					sha256: shared.reference.sha256,
				},
			],
		},
		artifact: {
			raw_path: `${sourceRoot}/provider-raw.png`,
			raw_sha256: hash(sourceBytes),
			media_type: 'image/png',
			width: 5,
			height: 5,
			children: Object.fromEntries(
				Object.entries(art).map(([kind, value]) => [
					kind,
					{
						raw_path: value.source_raw_path,
						candidate_sha256: value.sha256,
						raw_sha256: value.source_raw_sha256,
						candidate_path: value.source_candidate_path,
						media_type: 'image/png',
						width: value.width,
						height: value.height,
					},
				]),
			),
		},
	};
	const receiptBytes = Buffer.from(`${JSON.stringify(receiptValue)}\n`);
	const receipt = await promotedSource(
		root,
		releaseRoot,
		`${releasePersonRoot}/candidate-receipt.json`,
		`${sourceRoot}/candidate-receipt.json`,
		receiptBytes,
	);
	const decisionBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-human-review-decision',
		candidate_receipt_path: receipt.source_path,
		decision: 'approved',
		promotion_eligible: true,
		candidate_receipt_sha256: receipt.sha256,
		candidate_sha256: pairCandidateDigest(receiptValue),
		raw_sha256: hash(sourceBytes),
		subject,
		asset,
		hair_topology: topologyBinding(subject),
		marker_identity: markerBinding(subject),
		reviewer: 'fixture',
		reviewed_at: '2026-01-01T00:00:00.000Z',
		notes: 'fixture',
		checklist: Object.fromEntries(
			REQUIRED_PAIR_REVIEW_CHECKS.map((key) => [key, true]),
		),
		source_template_path: `${sourceRoot}/review-template.json`,
	};
	const decisionValue = {
		...decisionBase,
		review_id: computeReviewId(decisionBase),
	};
	const decision = await promotedSource(
		root,
		releaseRoot,
		`${releasePersonRoot}/review-decision.json`,
		`${sourceRoot}/review-decision.json`,
		Buffer.from(`${JSON.stringify(decisionValue)}\n`),
	);
	return {
		subject,
		job_id: jobId,
		candidate_index: 1,
		provider,
		generation: {
			receipt_path: receipt.path,
			receipt_sha256: receipt.sha256,
			source_receipt_path: receipt.source_path,
			prompt_path: `${releasePersonRoot}/prompt.txt`,
			prompt_file_sha256: hash(promptBytes),
			prompt_sha256: promptSha256,
			input_record_path: `${releasePersonRoot}/input-record.json`,
			input_record_file_sha256: hash(inputRecordBytes),
			input_record_sha256: hash(canonicalJson(inputRecord)),
			raw_artifact: {
				path: `${releasePersonRoot}/provider-raw.png`,
				sha256: hash(sourceBytes),
				source_path: `${sourceRoot}/provider-raw.png`,
			},
			reprocessing: null,
			timing: null,
		},
		art,
		approval: {
			decision_path: decision.path,
			decision_sha256: decision.sha256,
			source_decision_path: decision.source_path,
			pointer_path: `${sourceRoot}/review.json`,
			pointer_sha256: hash(promptBytes),
			review_id: decisionValue.review_id,
			decision: 'approved',
			promotion_eligible: true,
			reviewer: 'fixture',
			reviewed_at: '2026-01-01T00:00:00.000Z',
			notes: 'fixture',
			checklist: decisionValue.checklist,
			hair_topology_sha256: decisionValue.hair_topology.sha256,
			marker_identity_sha256: decisionValue.marker_identity.sha256,
		},
		_test: {
			receipt: receiptValue,
			receiptFile: receipt,
			decision: decisionValue,
		},
	};
}

async function fixture() {
	const root = await mkdtemp(join(tmpdir(), 'notebook-person-art-'));
	temporaryRoots.push(root);
	const releaseRoot = join(root, 'approved', 'v1');
	const portraitBytes = transparentArt(4, 4, [66, 48, 36]);
	const markerBytes = transparentArt(4, 8, [74, 91, 53], true);
	const sourceBytes = transparentArt(5, 5, [85, 66, 44]);
	const reference = await promotedSource(
		root,
		releaseRoot,
		'references/style.png',
		'source/style.png',
		sourceBytes,
	);
	const configValue = {
		pipeline_revision: 'fixture-v1',
		provider: {
			id: 'fixture',
			adapter: 'fixture',
			model: 'fixture',
			endpoint: '/fixture',
			request: {},
		},
		raw_output: { fixture: true },
		validation: { fixture: true },
		reference_inputs: [
			{
				id: 'style',
				path: 'source/style.png',
				purpose: 'fixture',
				asset_kinds: ['pair'],
			},
		],
	};
	const inputsValue = {
		npcs: [
			{ npc_id: 1, key: '1' },
			{ npc_id: 2, key: '2' },
		],
		fallback: { key: 'fallback' },
	};
	const config = await promotedSource(
		root,
		releaseRoot,
		'generation-config.json',
		'source/generation-config.json',
		Buffer.from(`${JSON.stringify(configValue)}\n`),
	);
	const inputs = await promotedSource(
		root,
		releaseRoot,
		'npc-art-inputs.json',
		'source/npc-art-inputs.json',
		Buffer.from(`${JSON.stringify(inputsValue)}\n`),
	);
	const shared = {
		config: { ...configValue, ...config },
		inputs,
		reference: { id: 'style', purpose: 'fixture', ...reference },
	};
	const entriesWithTest = [
		await makeEntry(
			root,
			releaseRoot,
			1,
			'Person One',
			portraitBytes,
			markerBytes,
			sourceBytes,
			shared,
		),
		await makeEntry(
			root,
			releaseRoot,
			2,
			'Person Two',
			portraitBytes,
			markerBytes,
			sourceBytes,
			shared,
		),
		await makeEntry(
			root,
			releaseRoot,
			null,
			'Unknown parish neighbour',
			portraitBytes,
			markerBytes,
			sourceBytes,
			shared,
		),
	];
	const castBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-whole-cast-human-review-decision',
		source_packets: [
			{
				path: 'review-packet/manifest.json',
				sha256: hash(Buffer.from('packet')),
			},
		],
		cast: {
			named_count: 2,
			total_count: 3,
			members: entriesWithTest.map((entry) => ({
				subject_key: subjectKey(entry.subject),
				subject: entry.subject,
				candidate_receipt_path: entry.generation.source_receipt_path,
				candidate_receipt_sha256: entry.generation.receipt_sha256,
				candidate_sha256: pairCandidateDigest(entry._test.receipt),
				raw_sha256: entry.generation.raw_artifact.sha256,
				portrait_sha256: entry.art.portrait.sha256,
				marker_sha256: entry.art.marker.sha256,
				hair_topology: entry._test.decision.hair_topology,
				marker_identity: entry._test.decision.marker_identity,
			})),
		},
		decision: 'approved',
		promotion_eligible: true,
		reviewer: 'fixture-cast-reviewer',
		reviewed_at: '2026-01-01T00:01:00.000Z',
		notes: 'fixture',
		checklist: {
			cast_distinctive: true,
			cast_hair_topology_distinctive: true,
		},
		source_template_path: 'review-packet/whole-cast-template.json',
	};
	const castReview = { ...castBase, review_id: computeReviewId(castBase) };
	const castReviewBytes = Buffer.from(`${JSON.stringify(castReview)}\n`);
	await write(join(releaseRoot, 'whole-cast-review.json'), castReviewBytes);
	const entries = entriesWithTest.map(({ _test, ...entry }) => entry);
	const manifestBase = {
		schema_version: 1,
		manifest_type: 'notebook-person-art-approved-release',
		release_version: 'v1',
		mode: 'fixture',
		entry_count: entries.length,
		approval: {
			whole_cast_visual_review: {
				path: 'whole-cast-review.json',
				sha256: hash(castReviewBytes),
				review_id: castReview.review_id,
			},
		},
		provenance: {
			generation_config: config,
			npc_art_inputs: inputs,
			references: [{ id: 'style', purpose: 'fixture', ...reference }],
		},
		entries,
	};
	const manifest = {
		...manifestBase,
		release_id: hash(canonicalJson(manifestBase)),
	};
	const releasePath = join(releaseRoot, 'release-manifest.json');
	await writeFile(releasePath, `${JSON.stringify(manifest, null, 2)}\n`);
	return { root, releaseRoot, releasePath, manifest };
}

async function rewriteManifest(data) {
	const { release_id: _releaseId, ...base } = data.manifest;
	data.manifest.release_id = hash(canonicalJson(base));
	await writeFile(
		data.releasePath,
		`${JSON.stringify(data.manifest, null, 2)}\n`,
	);
}

async function runBuild(data, runtimeName) {
	const runtimeRoot = join(data.root, runtimeName);
	await mkdir(runtimeRoot, { recursive: true });
	await writeFile(
		join(runtimeRoot, 'base-manifest.json'),
		`${JSON.stringify({ version: 2, assets: { retained: ['frame.png'] } }, null, 2)}\n`,
	);
	const result = await buildNotebookPersonArt({
		uiRoot: data.root,
		repoRoot: data.root,
		releaseManifestPath: data.releasePath,
		releaseRoot: data.releaseRoot,
		releaseManifestLabel: 'approved/v1/release-manifest.json',
		runtimeRoot,
		baseManifestPath: join(runtimeRoot, 'base-manifest.json'),
		expectedNamedCount: 2,
		allowFixture: true,
		runtimeSizes: {
			portrait: { width: 8, height: 8 },
			marker: { width: 8, height: 8 },
		},
	});
	return { result, runtimeRoot };
}

async function writeFreshnessFixture(root) {
	const releasePath = join(root, 'approved', 'v1', 'release-manifest.json');
	const runtimePath = join(root, 'runtime', 'asset-manifest.json');
	const entry = {
		subject: {
			kind: 'npc',
			npc_id: 1,
			name: 'Person One',
			input_record_sha256: 'a'.repeat(64),
		},
		job_id: 'b'.repeat(64),
		approval: { review_id: 'review-one' },
		art: {
			portrait: {
				master_path: 'people/1/portrait.png',
				sha256: 'c'.repeat(64),
				source_candidate_path: 'candidate/1/portrait.png',
				source_raw_path: 'candidate/1/portrait-raw.png',
				source_raw_sha256: 'd'.repeat(64),
			},
			marker: {
				master_path: 'people/1/marker.png',
				sha256: 'e'.repeat(64),
				source_candidate_path: 'candidate/1/marker.png',
				source_raw_path: 'candidate/1/marker-raw.png',
				source_raw_sha256: 'f'.repeat(64),
			},
		},
	};
	const fallback = structuredClone(entry);
	fallback.subject = {
		...fallback.subject,
		kind: 'fallback',
		npc_id: null,
		name: 'Unknown Neighbour',
	};
	fallback.job_id = '1'.repeat(64);
	fallback.approval.review_id = 'review-fallback';
	const releaseBase = { schema_version: 1, entries: [entry, fallback] };
	const release = {
		...releaseBase,
		release_id: hash(Buffer.from(canonicalJson(releaseBase))),
	};
	const releaseBytes = Buffer.from(`${JSON.stringify(release, null, 2)}\n`);
	await write(releasePath, releaseBytes);
	const releaseLabel = 'approved/v1/release-manifest.json';
	const record = (candidate) => ({
		npc_id: candidate.subject.npc_id,
		real_name: candidate.subject.name,
		display_name: candidate.subject.name,
		approval_status: 'approved',
		provenance: {
			release_manifest: releaseLabel,
			release_manifest_sha256: hash(releaseBytes),
			release_id: release.release_id,
			job_id: candidate.job_id,
			input_record_sha256: candidate.subject.input_record_sha256,
			review_id: candidate.approval.review_id,
			portrait: {
				master_path: candidate.art.portrait.master_path,
				master_sha256: candidate.art.portrait.sha256,
				source_candidate_path: candidate.art.portrait.source_candidate_path,
				source_raw_path: candidate.art.portrait.source_raw_path,
				source_raw_sha256: candidate.art.portrait.source_raw_sha256,
			},
			marker: {
				master_path: candidate.art.marker.master_path,
				master_sha256: candidate.art.marker.sha256,
				source_candidate_path: candidate.art.marker.source_candidate_path,
				source_raw_path: candidate.art.marker.source_raw_path,
				source_raw_sha256: candidate.art.marker.source_raw_sha256,
			},
		},
	});
	await write(
		runtimePath,
		Buffer.from(
			`${JSON.stringify(
				{
					assets: {
						personArt: {
							release_id: release.release_id,
							release_manifest: releaseLabel,
							release_manifest_sha256: hash(releaseBytes),
							approval_status: 'approved',
							people: [record(entry)],
							fallback: record(fallback),
						},
					},
				},
				null,
				2,
			)}\n`,
		),
	);
	return { releasePath, runtimePath };
}

async function treeDigest(root) {
	const paths = (await readdir(root, { recursive: true, withFileTypes: true }))
		.filter((entry) => entry.isFile() && entry.name !== 'base-manifest.json')
		.map((entry) => join(entry.parentPath, entry.name))
		.sort();
	const digest = createHash('sha256');
	for (const path of paths) {
		digest.update(relative(root, path));
		digest.update(await readFile(path));
	}
	return digest.digest('hex');
}

afterEach(async () => {
	await Promise.all(
		temporaryRoots
			.splice(0)
			.map((root) => rm(root, { recursive: true, force: true })),
	);
});

describe('approved notebook person art release builder', () => {
	test('accepts a runtime manifest bound to the approved release', async () => {
		const root = await mkdtemp(
			join(tmpdir(), 'notebook-person-art-freshness-'),
		);
		temporaryRoots.push(root);
		const { releasePath, runtimePath } = await writeFreshnessFixture(root);

		await expect(
			verifyNotebookPersonArtFreshness({
				repoRoot: root,
				releaseManifestPath: releasePath,
				runtimeManifestPath: runtimePath,
			}),
		).resolves.toMatchObject({ status: 'fresh', release_present: true });
	});

	test('rejects stale runtime provenance after an approved release changes', async () => {
		const root = await mkdtemp(join(tmpdir(), 'notebook-person-art-stale-'));
		temporaryRoots.push(root);
		const { releasePath, runtimePath } = await writeFreshnessFixture(root);
		const release = JSON.parse(await readFile(releasePath, 'utf8'));
		release.entries[0].approval.review_id = 'review-updated';
		const { release_id: _releaseId, ...releaseBase } = release;
		release.release_id = hash(Buffer.from(canonicalJson(releaseBase)));
		await writeFile(releasePath, `${JSON.stringify(release, null, 2)}\n`);

		await expect(
			verifyNotebookPersonArtFreshness({
				repoRoot: root,
				releaseManifestPath: releasePath,
				runtimeManifestPath: runtimePath,
			}),
		).rejects.toThrow(/release_id does not match/);
	});

	test('skips before a release exists but rejects a missing runtime manifest after release', async () => {
		const root = await mkdtemp(join(tmpdir(), 'notebook-person-art-missing-'));
		temporaryRoots.push(root);
		const releasePath = join(root, 'approved', 'v1', 'release-manifest.json');
		const runtimePath = join(root, 'runtime', 'asset-manifest.json');

		await expect(
			verifyNotebookPersonArtFreshness({
				repoRoot: root,
				releaseManifestPath: releasePath,
				runtimeManifestPath: runtimePath,
			}),
		).resolves.toEqual({ status: 'skipped', release_present: false });

		const fixture = await writeFreshnessFixture(root);
		await rm(fixture.runtimePath);
		await expect(
			verifyNotebookPersonArtFreshness({
				repoRoot: root,
				releaseManifestPath: fixture.releasePath,
				runtimeManifestPath: fixture.runtimePath,
			}),
		).rejects.toThrow(/runtime asset manifest is missing/);
	});

	test('rejects PNG masters with invalid chunk CRCs', () => {
		const corrupted = Buffer.from(transparentArt(4, 4, [66, 48, 36]));
		corrupted[29] ^= 0xff;

		expect(() => decodePngBytes(corrupted, 'corrupt fixture')).toThrow(
			/invalid IHDR chunk CRC/,
		);
	});

	test('reproducibly ships named pairs and fallback with a dynamic contact sheet', async () => {
		const data = await fixture();
		const first = await runBuild(data, 'runtime-a');
		const second = await runBuild(data, 'runtime-b');

		expect(first.result).toMatchObject({
			named_count: 2,
			fallback_count: 1,
			contact_sheet_rows: 1,
		});
		expect(await treeDigest(first.runtimeRoot)).toBe(
			await treeDigest(second.runtimeRoot),
		);
		const manifest = JSON.parse(
			await readFile(join(first.runtimeRoot, 'asset-manifest.json')),
		);
		expect(manifest.assets.retained).toEqual(['frame.png']);
		expect(manifest.assets.personArt.people).toHaveLength(2);
		expect(manifest.assets.personArt.people[0]).toMatchObject({
			npc_id: 1,
			portrait: 'people/portrait-person-one.png',
			marker: 'people/marker-person-one.png',
			approval_status: 'approved',
		});
		expect(manifest.assets.personArt.people[0].provenance).toMatchObject({
			release_manifest: 'approved/v1/release-manifest.json',
			release_id: data.manifest.release_id,
			portrait: { master_sha256: data.manifest.entries[0].art.portrait.sha256 },
			marker: {
				source_raw_sha256:
					data.manifest.entries[0].art.marker.source_raw_sha256,
			},
			review_id: data.manifest.entries[0].approval.review_id,
		});
		expect(manifest.assets.personArt.fallback).toMatchObject({
			npc_id: null,
			portrait: 'people/portrait-unknown-neighbour.png',
			marker: 'people/marker-unknown-neighbour.png',
		});
		const sheet = decodePngBytes(
			await readFile(join(first.runtimeRoot, 'person-art-contact-sheet.png')),
		);
		expect(sheet.height).toBe(298);
		expect(
			await readFile(
				join(first.runtimeRoot, 'person-art-contact-sheet.html'),
				'utf8',
			),
		).toContain('grid-template-columns: repeat(4');
	});

	test('contain-scales the complete marker without cropping its endpoints', async () => {
		const data = await fixture();
		const { runtimeRoot } = await runBuild(data, 'runtime');
		const marker = decodePngBytes(
			await readFile(join(runtimeRoot, 'people', 'marker-person-one.png')),
		);
		expect(marker).toMatchObject({ width: 8, height: 8 });
		expect(getPixel(marker, 4, 0)[3]).toBeGreaterThan(16);
		expect(getPixel(marker, 4, 7)[3]).toBeGreaterThan(16);
		expect(getPixel(marker, 0, 4)[3]).toBe(0);
		expect(getPixel(marker, 7, 4)[3]).toBe(0);
	});

	test('rebuilds from the approved release after local candidate sources are removed', async () => {
		const data = await fixture();
		await Promise.all([
			rm(join(data.root, 'candidate-source'), {
				recursive: true,
				force: true,
			}),
			rm(join(data.root, 'source'), { recursive: true, force: true }),
		]);

		const { result } = await runBuild(data, 'clean-checkout-runtime');

		expect(result).toMatchObject({ named_count: 2, fallback_count: 1 });
	});

	test('preserves the shipping tree when the base manifest is malformed', async () => {
		const data = await fixture();
		const runtimeRoot = join(data.root, 'existing-runtime');
		const sentinel = join(runtimeRoot, 'people', 'keep.txt');
		const baseManifest = join(runtimeRoot, 'asset-manifest.json');
		await write(sentinel, Buffer.from('existing shipping art'));
		await write(baseManifest, Buffer.from('{ malformed'));

		await expect(
			buildNotebookPersonArt({
				repoRoot: data.root,
				releaseManifestPath: data.releasePath,
				releaseRoot: data.releaseRoot,
				runtimeRoot,
				baseManifestPath: baseManifest,
				expectedNamedCount: 2,
				allowFixture: true,
			}),
		).rejects.toThrow();
		expect(await readFile(sentinel, 'utf8')).toBe('existing shipping art');
	});

	test.each([
		[
			'master hash',
			(data) => {
				data.manifest.entries[0].art.portrait.sha256 = '0'.repeat(64);
			},
			/master hash mismatch/,
		],
		[
			'source hash',
			(data) => {
				data.manifest.entries[0].art.marker.source_raw_sha256 = '0'.repeat(64);
			},
			/raw hash mismatch/,
		],
		[
			'dimensions',
			(data) => {
				data.manifest.entries[0].art.portrait.width = 9;
			},
			/dimensions mismatch/,
		],
	])(
		'fails before writing runtime assets on a %s mismatch',
		async (_name, mutate, expected) => {
			const data = await fixture();
			mutate(data);
			await rewriteManifest(data);
			const runtimeRoot = join(data.root, 'failed-runtime');
			await expect(
				buildNotebookPersonArt({
					repoRoot: data.root,
					releaseManifestPath: data.releasePath,
					releaseRoot: data.releaseRoot,
					runtimeRoot,
					baseManifestPath: null,
					expectedNamedCount: 2,
					allowFixture: true,
				}),
			).rejects.toThrow(expected);
			await expect(readdir(runtimeRoot)).rejects.toThrow();
		},
	);

	test.each([
		['opaque', createImage(4, 4, [40, 40, 40, 255]), /transparent background/],
		[
			'blank',
			createImage(4, 4, [0, 0, 0, 0]),
			/blank or has too few visible pixels/,
		],
	])('rejects a %s PNG master', async (_name, image, expected) => {
		const data = await fixture();
		const bytes = encodePng(image);
		const art = data.manifest.entries[0].art.portrait;
		await write(join(data.releaseRoot, art.master_path), bytes);
		await write(join(data.root, art.source_candidate_path), bytes);
		art.sha256 = hash(bytes);
		art.width = image.width;
		art.height = image.height;
		await rewriteManifest(data);
		await expect(
			buildNotebookPersonArt({
				repoRoot: data.root,
				releaseManifestPath: data.releasePath,
				releaseRoot: data.releaseRoot,
				runtimeRoot: join(data.root, 'failed-runtime'),
				baseManifestPath: null,
				expectedNamedCount: 2,
				allowFixture: true,
			}),
		).rejects.toThrow(expected);
	});

	test('rejects a release ID that does not bind the manifest content', async () => {
		const data = await fixture();
		data.manifest.release_id = '0'.repeat(64);
		await writeFile(
			data.releasePath,
			`${JSON.stringify(data.manifest, null, 2)}\n`,
		);
		await expect(
			buildNotebookPersonArt({
				repoRoot: data.root,
				releaseManifestPath: data.releasePath,
				releaseRoot: data.releaseRoot,
				runtimeRoot: join(data.root, 'runtime'),
				baseManifestPath: null,
				expectedNamedCount: 2,
				allowFixture: true,
			}),
		).rejects.toThrow('release_id mismatch');
	});

	test('rejects an invented pair review id even when the manifest is rehashed', async () => {
		const data = await fixture();
		data.manifest.entries[0].approval.review_id = '0'.repeat(24);
		await rewriteManifest(data);
		await expect(runBuild(data, 'invented-review-runtime')).rejects.toThrow(
			/decision does not match release entry/,
		);
	});

	test('rejects an invented whole-cast review id with refreshed file hashes', async () => {
		const data = await fixture();
		const descriptor = data.manifest.approval.whole_cast_visual_review;
		const path = join(data.releaseRoot, descriptor.path);
		const record = JSON.parse(await readFile(path, 'utf8'));
		record.review_id = 'f'.repeat(24);
		const bytes = Buffer.from(`${JSON.stringify(record)}\n`);
		await write(path, bytes);
		descriptor.sha256 = hash(bytes);
		descriptor.review_id = record.review_id;
		await rewriteManifest(data);
		await expect(
			runBuild(data, 'invented-cast-review-runtime'),
		).rejects.toThrow(/whole-cast review identity or approval is invalid/);
	});

	test('rejects a decision missing an exact checklist key after coordinated rehashing', async () => {
		const data = await fixture();
		const entry = data.manifest.entries[0];
		const decisionPath = join(data.releaseRoot, entry.approval.decision_path);
		const decision = JSON.parse(await readFile(decisionPath, 'utf8'));
		delete decision.checklist.production_quality;
		decision.review_id = computeReviewId(decision);
		const decisionBytes = Buffer.from(`${JSON.stringify(decision)}\n`);
		await write(decisionPath, decisionBytes);
		entry.approval.decision_sha256 = hash(decisionBytes);
		entry.approval.review_id = decision.review_id;
		entry.approval.checklist = decision.checklist;
		await rewriteManifest(data);
		await expect(runBuild(data, 'missing-check-runtime')).rejects.toThrow(
			/checklist keys do not match the approval contract/,
		);
	});

	test.each([
		[
			'master',
			async (data) => {
				const art = data.manifest.entries[0].art.portrait;
				const bytes = transparentArt(4, 4, [120, 80, 40]);
				await write(join(data.releaseRoot, art.master_path), bytes);
				art.sha256 = hash(bytes);
			},
			/provenance does not match receipt/,
		],
		[
			'provider raw',
			async (data) => {
				const raw = data.manifest.entries[0].generation.raw_artifact;
				const bytes = transparentArt(5, 5, [100, 70, 50]);
				await write(join(data.releaseRoot, raw.path), bytes);
				raw.sha256 = hash(bytes);
			},
			/provider raw hash does not match receipt/,
		],
		[
			'decision',
			async (data) => {
				const approval = data.manifest.entries[0].approval;
				const path = join(data.releaseRoot, approval.decision_path);
				const decision = JSON.parse(await readFile(path, 'utf8'));
				decision.notes = 'Tampered after review.';
				const bytes = Buffer.from(`${JSON.stringify(decision)}\n`);
				await write(path, bytes);
				approval.decision_sha256 = hash(bytes);
				approval.notes = decision.notes;
			},
			/decision does not match release entry/,
		],
	])(
		'rejects coordinated manifest tampering around %s bytes',
		async (_name, mutate, expected) => {
			const data = await fixture();
			await mutate(data);
			await rewriteManifest(data);
			await expect(runBuild(data, `tampered-${_name}-runtime`)).rejects.toThrow(
				expected,
			);
		},
	);

	test('requires exactly 23 named people by default', async () => {
		const data = await fixture();
		await expect(
			buildNotebookPersonArt({
				repoRoot: data.root,
				releaseManifestPath: data.releasePath,
				releaseRoot: data.releaseRoot,
				runtimeRoot: join(data.root, 'runtime'),
				baseManifestPath: null,
				allowFixture: true,
			}),
		).rejects.toThrow('exactly 23 named people plus one fallback');
	});
});
