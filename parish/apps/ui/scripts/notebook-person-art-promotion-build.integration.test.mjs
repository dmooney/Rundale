// @vitest-environment node

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { afterEach, test } from 'vitest';

import { buildNotebookPersonArt } from './build-notebook-person-art.mjs';
import {
	computeReviewId,
	hairTopologyBinding,
	markerIdentityBinding,
	pairCandidateDigest,
	subjectKey,
} from './notebook-person-art-approval-contract.mjs';
import {
	promoteNotebookPersonArt,
	REQUIRED_PAIR_REVIEW_CHECKS,
} from './promote-notebook-person-art.mjs';
import {
	createImage,
	encodePng,
	setPixel,
} from './notebook-person-art-png.mjs';

const cleanupRoots = [];
const canonicalConfigPath =
	'parish/apps/ui/art/notebook-person-art/generation-config-v1.json';
const canonicalInputsPath =
	'parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json';
const canonicalArtDirectionPath =
	'parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json';

afterEach(async () => {
	await Promise.all(
		cleanupRoots
			.splice(0)
			.map((root) => rm(root, { recursive: true, force: true })),
	);
});

function hash(value) {
	return createHash('sha256').update(value).digest('hex');
}

function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
	return `{${Object.entries(value)
		.toSorted(([left], [right]) => left.localeCompare(right))
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

function json(value) {
	return `${JSON.stringify(value, null, '\t')}\n`;
}

async function write(path, value) {
	await mkdir(dirname(path), { recursive: true });
	await writeFile(path, value);
}

function uniqueTransparentPng(seed, { marker = false } = {}) {
	const image = createImage(12, 12);
	const color = [40 + seed * 3, 52 + seed * 5, 64 + seed * 7, 255];
	if (marker) {
		for (let y = 1; y < 11; y += 1) {
			setPixel(image, 3 + (seed % 5), y, color);
		}
	} else {
		for (let y = 2; y < 10; y += 1) {
			for (let x = 2; x < 10; x += 1) {
				if ((x + y + seed) % 3 !== 0) setPixel(image, x, y, color);
			}
		}
	}
	return encodePng(image);
}

function subjectFor(index) {
	return index === null
		? {
				kind: 'fallback',
				npc_id: null,
				name: 'Unknown parish neighbour',
			}
		: { kind: 'npc', npc_id: index, name: `Synthetic Person ${index}` };
}

function inputFor(subject) {
	return {
		...(subject.npc_id === null ? {} : { npc_id: subject.npc_id }),
		name: subject.name,
		pair_prompt: `Synthetic paired art for ${subject.name}.`,
		portrait_prompt: `Synthetic portrait for ${subject.name}.`,
		marker_prompt: `Synthetic marker for ${subject.name}.`,
	};
}

function topologyFor(subject) {
	const seed = subject.npc_id ?? 'fallback';
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

function markerIdentityFor(subject) {
	const seed = subject.npc_id ?? 'fallback';
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

async function installFixtureArtInputsGenerator(
	root,
	{ inputsBytes, artDirectionBytes },
) {
	const crateRoot = join(root, 'parish', 'crates', 'parish-npc-tool');
	const fixtureRoot = join(crateRoot, 'freshness-fixtures');
	const npcsBytes = Buffer.from('{"fixture":"canonical npcs"}\n');
	const worldBytes = Buffer.from('{"fixture":"canonical world"}\n');
	const cargoManifest = `[workspace]
members = ["crates/parish-npc-tool"]
resolver = "2"
`;
	const crateManifest = `[package]
name = "parish-npc-tool"
version = "0.0.0"
edition = "2021"
`;
	const main = `use std::{env, fs, path::{Path, PathBuf}};

fn arg_value(args: &[String], flag: &str) -> PathBuf {
    let index = args.iter().position(|arg| arg == flag).expect("missing flag");
    PathBuf::from(args.get(index + 1).expect("missing flag value"))
}

fn same(left: &Path, right: &Path) -> bool {
    fs::read(left).ok() == fs::read(right).ok()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("freshness-fixtures");
    let fresh = same(&arg_value(&args, "--npcs"), &fixtures.join("npcs.json"))
        && same(&arg_value(&args, "--world"), &fixtures.join("world.json"))
        && same(
            &arg_value(&args, "--art-direction"),
            &fixtures.join("npc-art-direction-v1.json"),
        );
    let mut output = fs::read(fixtures.join("npc-art-inputs-v1.json")).unwrap();
    if !fresh {
        output.extend_from_slice(b" ");
    }
    fs::write(arg_value(&args, "--output"), output).unwrap();
}
`;
	await Promise.all([
		write(join(root, 'parish', 'Cargo.toml'), cargoManifest),
		write(join(crateRoot, 'Cargo.toml'), crateManifest),
		write(join(crateRoot, 'src', 'main.rs'), main),
		write(join(root, 'mods', 'rundale', 'npcs.json'), npcsBytes),
		write(join(root, 'mods', 'rundale', 'world.json'), worldBytes),
		write(join(fixtureRoot, 'npcs.json'), npcsBytes),
		write(join(fixtureRoot, 'world.json'), worldBytes),
		write(join(fixtureRoot, 'npc-art-direction-v1.json'), artDirectionBytes),
		write(join(fixtureRoot, 'npc-art-inputs-v1.json'), inputsBytes),
	]);
}

async function createCandidate(root, subject, input, shared) {
	const key = subject.npc_id === null ? 'fallback' : String(subject.npc_id);
	const candidateRoot = join(
		root,
		'candidate-store',
		'objects',
		'fixture',
		key,
	);
	const attemptRoot = join(candidateRoot, 'attempts', 'attempt-1');
	const receiptPath = join(candidateRoot, 'receipt.json');
	const decisionPath = join(attemptRoot, 'reviews', 'decision.json');
	const pointerPath = join(candidateRoot, 'review.json');
	const promptPath = join(candidateRoot, 'prompt.txt');
	const inputRecordPath = join(candidateRoot, 'input-record.json');
	const providerRawPath = join(attemptRoot, 'raw.png');
	const portraitRawPath = join(attemptRoot, 'portrait-raw.png');
	const portraitPath = join(attemptRoot, 'portrait-candidate.png');
	const markerRawPath = join(attemptRoot, 'marker-raw.png');
	const markerPath = join(attemptRoot, 'marker-candidate.png');
	const seed = subject.npc_id ?? 24;
	const portrait = uniqueTransparentPng(seed);
	const marker = uniqueTransparentPng(seed, { marker: true });
	const providerRaw = uniqueTransparentPng(seed + 48);
	const portraitRaw = uniqueTransparentPng(seed + 72);
	const markerRaw = uniqueTransparentPng(seed + 96, { marker: true });
	const prompt = `Synthetic provider prompt for ${subject.name}`;
	const inputRecord = json(input);

	await Promise.all([
		write(promptPath, `${prompt}\n`),
		write(inputRecordPath, inputRecord),
		write(providerRawPath, providerRaw),
		write(portraitRawPath, portraitRaw),
		write(portraitPath, portrait),
		write(markerRawPath, markerRaw),
		write(markerPath, marker),
	]);

	const fullSubject = {
		...subject,
		input_record_sha256: hash(canonicalJson(input)),
	};
	const asset = {
		kind: 'pair',
		candidate_index: 1,
		children: ['portrait', 'marker'],
	};
	const identity = {
		schema_version: 1,
		pipeline_revision: shared.config.pipeline_revision,
		provider: {
			id: shared.config.provider.id,
			adapter: shared.config.provider.adapter,
			model: shared.config.provider.model,
			request: shared.config.provider.request,
		},
		raw_output: shared.config.raw_output,
		validation: shared.config.validation,
		reference_inputs: [
			{ id: shared.reference.id, sha256: shared.reference.sha256 },
		],
		subject_kind: fullSubject.kind,
		npc_id: fullSubject.npc_id,
		input_record_sha256: fullSubject.input_record_sha256,
		asset_kind: 'pair',
		candidate_index: asset.candidate_index,
		prompt_sha256: hash(prompt),
	};
	const children = {
		portrait: {
			raw_path: portraitRawPath,
			raw_sha256: hash(portraitRaw),
			candidate_path: portraitPath,
			candidate_sha256: hash(portrait),
			media_type: 'image/png',
			width: 12,
			height: 12,
			raw_validation: { synthetic: true },
			candidate_validation: { synthetic: true },
		},
		marker: {
			raw_path: markerRawPath,
			raw_sha256: hash(markerRaw),
			candidate_path: markerPath,
			candidate_sha256: hash(marker),
			media_type: 'image/png',
			width: 12,
			height: 12,
			raw_validation: { synthetic: true },
			candidate_validation: { synthetic: true },
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
		subject: fullSubject,
		asset,
		provider: {
			...shared.config.provider,
			base_url: 'https://fixture.invalid/v1',
			request_id: `fixture-request-${key}`,
			provider_created_at: 1,
			attempts: 1,
			usage: { total_tokens: 0 },
		},
		provenance: {
			config_path: canonicalConfigPath,
			config_sha256: shared.configSha256,
			inputs_path: canonicalInputsPath,
			inputs_sha256: shared.inputsSha256,
			prompt_path: promptPath,
			prompt_sha256: hash(prompt),
			input_record_path: inputRecordPath,
			reference_inputs: [shared.reference],
		},
		artifact: {
			raw_path: providerRawPath,
			raw_sha256: hash(providerRaw),
			media_type: 'image/png',
			width: 12,
			height: 12,
			children,
		},
		reprocessing: null,
		timing: {
			started_at: '2026-07-12T00:00:00.000Z',
			completed_at: '2026-07-12T00:00:01.000Z',
		},
	};
	await write(receiptPath, json(receipt));
	const receiptBytes = await readFile(receiptPath);
	const candidateSha256 = hash(
		JSON.stringify({
			portrait: children.portrait.candidate_sha256,
			marker: children.marker.candidate_sha256,
		}),
	);
	const decisionBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-human-review-decision',
		candidate_receipt_path: receiptPath,
		candidate_receipt_sha256: hash(receiptBytes),
		candidate_sha256: candidateSha256,
		raw_sha256: receipt.artifact.raw_sha256,
		subject: fullSubject,
		asset,
		hair_topology: hairTopologyBinding(shared.artDirection, fullSubject),
		marker_identity: markerIdentityBinding(shared.artDirection, fullSubject),
		decision: 'approved',
		promotion_eligible: true,
		reviewer: 'synthetic-reviewer',
		reviewed_at: '2026-07-12T00:01:00.000Z',
		notes: 'Synthetic immutable approval.',
		checklist: Object.fromEntries(
			REQUIRED_PAIR_REVIEW_CHECKS.map((check) => [check, true]),
		),
		source_template_path: join(root, 'synthetic-review-template.json'),
	};
	const decision = {
		...decisionBase,
		review_id: hash(JSON.stringify(decisionBase)).slice(0, 24),
	};
	await write(decisionPath, json(decision));
	const decisionBytes = await readFile(decisionPath);
	await write(
		pointerPath,
		json({
			schema_version: 1,
			candidate_sha256: candidateSha256,
			decision: 'approved',
			promotion_eligible: true,
			record_path: decisionPath,
			record_sha256: hash(decisionBytes),
		}),
	);
	return { receiptPath, decisionPath };
}

async function createProductionFixture() {
	const root = await mkdtemp(join(tmpdir(), 'rundale-art-promotion-build-'));
	cleanupRoots.push(root);
	const config = {
		schema_version: 1,
		pipeline_revision: 'synthetic-production-pairs-v1',
		provider: {
			id: 'fixture-provider',
			adapter: 'fixture-adapter',
			model: 'fixture-model-v1',
			endpoint: '/fixture',
			request: { size: '12x12', output_format: 'png' },
		},
		reference_inputs: [
			{
				id: 'fixture-style',
				path: 'candidate-store/references/style.png',
				purpose: 'Synthetic style fixture.',
				asset_kinds: ['pair'],
			},
		],
		raw_output: { postprocess_revision: 'fixture-v1' },
		validation: { synthetic: true },
	};
	const subjects = [
		...Array.from({ length: 23 }, (_, index) => subjectFor(index + 1)),
		subjectFor(null),
	];
	const inputs = {
		schema_version: 1,
		fallback: inputFor(subjects.at(-1)),
		npcs: subjects.slice(0, -1).map(inputFor),
	};
	const artDirection = {
		schema_version: 4,
		npcs: subjects.slice(0, -1).map((subject) => ({
			npc_id: subject.npc_id,
			portrait_identity: { hair_topology: topologyFor(subject) },
			marker_identity: markerIdentityFor(subject),
		})),
		fallback: {
			portrait_identity: { hair_topology: topologyFor(subjects.at(-1)) },
			marker_identity: markerIdentityFor(subjects.at(-1)),
		},
	};
	const configBytes = Buffer.from(json(config));
	const inputsBytes = Buffer.from(json(inputs));
	const artDirectionBytes = Buffer.from(json(artDirection));
	const referenceBytes = uniqueTransparentPng(120);
	const referencePath = join(
		root,
		'candidate-store',
		'references',
		'style.png',
	);
	await Promise.all([
		write(join(root, canonicalConfigPath), configBytes),
		write(join(root, canonicalInputsPath), inputsBytes),
		write(join(root, canonicalArtDirectionPath), artDirectionBytes),
		write(referencePath, referenceBytes),
	]);
	await installFixtureArtInputsGenerator(root, {
		inputsBytes,
		artDirectionBytes,
	});
	const shared = {
		config,
		artDirection,
		configSha256: hash(configBytes),
		inputsSha256: hash(inputsBytes),
		reference: {
			id: 'fixture-style',
			path: config.reference_inputs[0].path,
			purpose: 'Synthetic style fixture.',
			sha256: hash(referenceBytes),
		},
	};
	const candidates = await Promise.all(
		subjects.map((subject) =>
			createCandidate(root, subject, inputFor(subject), shared),
		),
	);
	const members = [];
	for (const candidate of candidates) {
		const receiptBytes = await readFile(candidate.receiptPath);
		const receipt = JSON.parse(receiptBytes);
		const decision = JSON.parse(await readFile(candidate.decisionPath));
		members.push({
			subject_key: subjectKey(receipt.subject),
			subject: receipt.subject,
			candidate_receipt_path: candidate.receiptPath,
			candidate_receipt_sha256: hash(receiptBytes),
			candidate_sha256: pairCandidateDigest(receipt),
			raw_sha256: receipt.artifact.raw_sha256,
			portrait_sha256: receipt.artifact.children.portrait.candidate_sha256,
			marker_sha256: receipt.artifact.children.marker.candidate_sha256,
			hair_topology: decision.hair_topology,
			marker_identity: decision.marker_identity,
		});
	}
	const sourcePackets = [];
	for (let index = 0; index < 3; index += 1) {
		const path = join(root, 'review-packets', `packet-${index + 1}.json`);
		const packet = {
			schema_version: 1,
			packet_id: `packet-${index + 1}`,
			candidate_count: 8,
			receipts: candidates
				.slice(index * 8, index * 8 + 8)
				.map((candidate) => candidate.receiptPath),
		};
		await write(path, json(packet));
		sourcePackets.push({ path, sha256: hash(await readFile(path)) });
	}
	const castBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-whole-cast-human-review-decision',
		source_packets: sourcePackets,
		cast: { named_count: 23, total_count: 24, members },
		decision: 'approved',
		promotion_eligible: true,
		reviewer: 'synthetic-whole-cast-reviewer',
		reviewed_at: '2026-07-12T00:02:00.000Z',
		notes: 'Synthetic review of three bounded eight-person packets.',
		checklist: {
			cast_distinctive: true,
			cast_hair_topology_distinctive: true,
		},
		source_template_path: join(root, 'whole-cast-review-template.json'),
	};
	const castReviewPath = join(root, 'whole-cast-review.json');
	await write(
		castReviewPath,
		json({ ...castBase, review_id: computeReviewId(castBase) }),
	);
	return { root, candidates, castReviewPath };
}

test('production refuses an alternate input catalog even when its bytes match canonical', async () => {
	const fixture = await createProductionFixture();
	const currentInputsPath = join(
		fixture.root,
		'current',
		'npc-art-inputs-v1.json',
	);
	const currentInputsBytes = await readFile(
		join(fixture.root, canonicalInputsPath),
	);
	await write(currentInputsPath, currentInputsBytes);

	const releaseRoot = join(fixture.root, 'approved', 'historical-catalog-v1');
	await assert.rejects(
		promoteNotebookPersonArt({
			repoRoot: fixture.root,
			receiptPaths: fixture.candidates.map(({ receiptPath }) => receiptPath),
			decisionPaths: fixture.candidates.map(({ decisionPath }) => decisionPath),
			castReviewPath: fixture.castReviewPath,
			inputsPath: currentInputsPath,
			outputDir: releaseRoot,
			mode: 'production',
		}),
		/repository canonical NPC art inputs catalog/,
	);
});

// Production freshness invokes Cargo with its own five-minute timeout.
test('promotes a complete approved production release that builds without its candidate store', async () => {
	const fixture = await createProductionFixture();
	const releaseRoot = join(fixture.root, 'approved', 'v1');
	const promoted = await promoteNotebookPersonArt({
		repoRoot: fixture.root,
		receiptPaths: fixture.candidates.map(({ receiptPath }) => receiptPath),
		decisionPaths: fixture.candidates.map(({ decisionPath }) => decisionPath),
		castReviewPath: fixture.castReviewPath,
		outputDir: releaseRoot,
		mode: 'production',
	});

	assert.equal(promoted.manifest.entry_count, 24);
	assert.equal(promoted.manifest.mode, 'production');
	assert.equal(
		promoted.manifest.provenance.npc_art_inputs.sha256,
		hash(await readFile(join(fixture.root, canonicalInputsPath))),
	);
	assert.deepEqual(
		await readFile(join(releaseRoot, 'npc-art-inputs.json')),
		await readFile(join(fixture.root, canonicalInputsPath)),
	);
	assert.deepEqual(
		promoted.manifest.entries.slice(0, -1).map((entry) => entry.subject.npc_id),
		Array.from({ length: 23 }, (_, index) => index + 1),
	);
	assert.equal(promoted.manifest.entries.at(-1).subject.npc_id, null);
	for (const entry of promoted.manifest.entries) {
		const key = entry.subject.npc_id ?? 'fallback';
		const receipt = JSON.parse(
			await readFile(
				join(releaseRoot, 'people', String(key), 'candidate-receipt.json'),
			),
		);
		assert.equal(receipt.hair_topology, undefined);
		assert.equal(receipt.subject.hair_topology, undefined);
		assert.equal(receipt.provider.request.hair_topology, undefined);
		assert.equal(
			hash(
				await readFile(
					join(releaseRoot, 'people', String(key), 'candidate-receipt.json'),
				),
			),
			entry.generation.receipt_sha256,
		);
		assert.equal(
			hash(
				await readFile(
					join(releaseRoot, 'people', String(key), 'review-decision.json'),
				),
			),
			entry.approval.decision_sha256,
		);
	}

	await rm(join(fixture.root, 'candidate-store'), {
		recursive: true,
		force: true,
	});
	const runtimeRoot = join(fixture.root, 'runtime');
	await write(
		join(runtimeRoot, 'base-manifest.json'),
		json({ version: 2, assets: { retained: ['frame.png'] } }),
	);
	const built = await buildNotebookPersonArt({
		repoRoot: fixture.root,
		releaseManifestPath: promoted.manifestPath,
		releaseRoot,
		runtimeRoot,
		baseManifestPath: join(runtimeRoot, 'base-manifest.json'),
		runtimeSizes: {
			portrait: { width: 12, height: 12 },
			marker: { width: 12, height: 12 },
		},
	});
	const runtimeManifest = JSON.parse(
		await readFile(join(runtimeRoot, 'asset-manifest.json'), 'utf8'),
	);
	assert.equal(
		JSON.stringify(runtimeManifest).includes('hair_topology'),
		false,
	);
	const entries = [
		...runtimeManifest.assets.personArt.people,
		runtimeManifest.assets.personArt.fallback,
	];
	const assetPaths = entries.flatMap(({ portrait, marker }) => [
		portrait,
		marker,
	]);
	const assetHashes = await Promise.all(
		assetPaths.map(async (path) =>
			hash(await readFile(join(runtimeRoot, path))),
		),
	);

	assert.equal(built.named_count, 23);
	assert.equal(built.fallback_count, 1);
	assert.equal(built.contact_sheet_rows, 6);
	assert.equal(built.runtime_root, runtimeRoot);
	assert.equal(built.release_id, promoted.manifest.release_id);
	assert.match(built.release_manifest_sha256, /^[a-f0-9]{64}$/);
	assert.equal(entries.length, 24);
	assert.equal(new Set(assetPaths).size, 48);
	assert.equal(new Set(assetHashes).size, 48);
}, 360_000);

test('production promotion rejects stale canonical catalogs and source mutations', async () => {
	const fixture = await createProductionFixture();
	const promote = (suffix) =>
		promoteNotebookPersonArt({
			repoRoot: fixture.root,
			receiptPaths: fixture.candidates.map(({ receiptPath }) => receiptPath),
			decisionPaths: fixture.candidates.map(({ decisionPath }) => decisionPath),
			castReviewPath: fixture.castReviewPath,
			outputDir: join(fixture.root, 'approved', suffix),
			mode: 'production',
		});

	const inputsPath = join(fixture.root, canonicalInputsPath);
	const inputs = await readFile(inputsPath);
	await write(inputsPath, Buffer.concat([inputs, Buffer.from('\n')]));
	await assert.rejects(
		promote('stale-inputs'),
		/Production candidate canonical NPC art inputs hash does not match/,
	);
	await write(inputsPath, inputs);

	for (const [label, sourcePath] of [
		['npcs', join(fixture.root, 'mods/rundale/npcs.json')],
		['world', join(fixture.root, 'mods/rundale/world.json')],
		['art-direction', join(fixture.root, canonicalArtDirectionPath)],
	]) {
		const source = await readFile(sourcePath);
		await write(sourcePath, Buffer.concat([source, Buffer.from('\n')]));
		await assert.rejects(
			promote(`stale-${label}`),
			/Canonical npc-art-inputs-v1\.json is stale against canonical NPC, world, or art-direction sources/,
		);
		await write(sourcePath, source);
	}
}, 30_000);

test('production promotion fails on missing or mutated current topology', async () => {
	const missing = await createProductionFixture();
	await rm(join(missing.root, canonicalArtDirectionPath));
	await assert.rejects(
		promoteNotebookPersonArt({
			repoRoot: missing.root,
			receiptPaths: missing.candidates.map(({ receiptPath }) => receiptPath),
			decisionPaths: missing.candidates.map(({ decisionPath }) => decisionPath),
			castReviewPath: missing.castReviewPath,
			outputDir: join(missing.root, 'approved', 'missing-topology'),
			mode: 'production',
		}),
		/Could not read current schema-v4 NPC art direction sidecar/,
	);

	const mutated = await createProductionFixture();
	const sidecarPath = join(mutated.root, canonicalArtDirectionPath);
	const sidecar = JSON.parse(await readFile(sidecarPath, 'utf8'));
	sidecar.npcs[0].portrait_identity.hair_topology.front.family =
		'mutated-production-front';
	await write(sidecarPath, json(sidecar));
	await assert.rejects(
		promoteNotebookPersonArt({
			repoRoot: mutated.root,
			receiptPaths: mutated.candidates.map(({ receiptPath }) => receiptPath),
			decisionPaths: mutated.candidates.map(({ decisionPath }) => decisionPath),
			castReviewPath: mutated.castReviewPath,
			outputDir: join(mutated.root, 'approved', 'mutated-topology'),
			mode: 'production',
		}),
		/Pair review decision hair topology does not match current schema-v4 hair topology/,
	);
});
