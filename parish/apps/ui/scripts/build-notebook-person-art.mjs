import { randomUUID } from 'node:crypto';
import {
	cp,
	mkdir,
	readFile,
	rename,
	rm,
	stat,
	writeFile,
} from 'node:fs/promises';
import {
	basename,
	dirname,
	isAbsolute,
	join,
	relative,
	resolve,
	sep,
} from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import {
	assertCanonicalHairTopologyBinding,
	assertCanonicalMarkerIdentityBinding,
	assertExactChecklist,
	canonicalJson,
	computeReviewId,
	NAMED_CAST_SIZE,
	pairCandidateDigest,
	REQUIRED_PAIR_REVIEW_CHECKS,
	REQUIRED_WHOLE_CAST_REVIEW_CHECKS,
	sha256,
	subjectKey,
} from './notebook-person-art-approval-contract.mjs';
import {
	alphaStats,
	compositeOpaque,
	createImage,
	decodePngBytes,
	encodePng,
	fillRect,
	resizeContain,
} from './notebook-person-art-png.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const defaultUiRoot = resolve(here, '..');
const defaultRepoRoot = resolve(defaultUiRoot, '../../..');
const RELEASE_MANIFEST_TYPE = 'notebook-person-art-approved-release';
const CONTACT_COLUMNS = 4;
const DEFAULT_RUNTIME_SIZES = {
	portrait: { width: 144, height: 164 },
	marker: { width: 120, height: 170 },
};

function promptPayloadHash(bytes) {
	return sha256(
		bytes.length > 0 && bytes.at(-1) === 0x0a ? bytes.subarray(0, -1) : bytes,
	);
}

function assertObject(value, label) {
	if (!value || typeof value !== 'object' || Array.isArray(value))
		throw new Error(`${label} must be an object`);
	return value;
}

function assertExactObjectKeys(value, expected, label) {
	assertObject(value, label);
	const actual = Object.keys(value).toSorted();
	const wanted = [...expected].toSorted();
	if (canonicalJson(actual) !== canonicalJson(wanted)) {
		throw new Error(`${label} keys do not match the approval contract`);
	}
}

function assertString(value, label) {
	if (typeof value !== 'string' || value.length === 0)
		throw new Error(`${label} must be a non-empty string`);
	return value;
}

function assertSha256(value, label) {
	if (!/^[a-f0-9]{64}$/.test(value ?? ''))
		throw new Error(`${label} must be a lowercase SHA-256 hash`);
	return value;
}

function portablePath(path) {
	return path.split(sep).join('/');
}

async function pathExists(path) {
	try {
		await stat(path);
		return true;
	} catch (error) {
		if (error.code === 'ENOENT') return false;
		throw error;
	}
}

async function replaceRuntimeTree(stagingRoot, runtimeRoot) {
	const backupRoot = join(
		dirname(runtimeRoot),
		`.${basename(runtimeRoot)}.backup-${process.pid}-${randomUUID()}`,
	);
	const hadRuntime = await pathExists(runtimeRoot);
	if (hadRuntime) await rename(runtimeRoot, backupRoot);
	try {
		await rename(stagingRoot, runtimeRoot);
	} catch (error) {
		if (hadRuntime) await rename(backupRoot, runtimeRoot);
		throw error;
	}
	if (hadRuntime) await rm(backupRoot, { recursive: true, force: true });
}

function containedPath(root, path, label) {
	assertString(path, label);
	if (isAbsolute(path)) throw new Error(`${label} must be relative: ${path}`);
	const target = resolve(root, path);
	const relation = relative(root, target);
	if (
		relation === '..' ||
		relation.startsWith(`..${sep}`) ||
		isAbsolute(relation)
	) {
		throw new Error(`${label} escapes its configured root: ${path}`);
	}
	return target;
}

function positiveInteger(value, label) {
	if (!Number.isInteger(value) || value <= 0)
		throw new Error(`${label} must be a positive integer`);
	return value;
}

function dimensions(value, label) {
	assertObject(value, label);
	return {
		width: positiveInteger(value.width, `${label}.width`),
		height: positiveInteger(value.height, `${label}.height`),
	};
}

function slugify(value) {
	const slug = value
		.normalize('NFKD')
		.replaceAll(/[\u0300-\u036f]/g, '')
		.toLowerCase()
		.replaceAll(/[^a-z0-9]+/g, '-')
		.replaceAll(/^-|-$/g, '');
	if (!slug) throw new Error(`Could not derive a runtime slug from ${value}`);
	return slug;
}

async function readVerifiedPath(root, configuredPath, expectedHash, label) {
	const path = containedPath(root, configuredPath, `${label}.path`);
	const bytes = await readFile(path);
	const actual = sha256(bytes);
	if (actual !== assertSha256(expectedHash, `${label}.sha256`)) {
		throw new Error(
			`${label} hash mismatch: expected ${expectedHash}, got ${actual}`,
		);
	}
	return { path, bytes, sha256: actual };
}

function validatePng(bytes, expectedDimensions, label, requireTransparency) {
	const image = decodePngBytes(bytes, label);
	if (
		expectedDimensions &&
		(image.width !== expectedDimensions.width ||
			image.height !== expectedDimensions.height)
	) {
		throw new Error(
			`${label} dimensions mismatch: expected ${expectedDimensions.width}x${expectedDimensions.height}, got ${image.width}x${image.height}`,
		);
	}
	const stats = alphaStats(image);
	const minimumPixels = Math.max(1, Math.ceil(stats.total * 0.001));
	if (stats.visible < minimumPixels)
		throw new Error(`${label} is blank or has too few visible pixels`);
	if (requireTransparency && stats.transparent < minimumPixels) {
		throw new Error(`${label} must have a transparent background`);
	}
	return image;
}

async function verifyPromotedFile(releaseRoot, record, label) {
	assertObject(record, label);
	return readVerifiedPath(releaseRoot, record.path, record.sha256, label);
}

async function verifyReleaseProvenance(manifest, releaseRoot) {
	const provenance = assertObject(manifest.provenance, 'release provenance');
	const configFile = await verifyPromotedFile(
		releaseRoot,
		provenance.generation_config,
		'generation config',
	);
	const inputsFile = await verifyPromotedFile(
		releaseRoot,
		provenance.npc_art_inputs,
		'NPC art inputs',
	);
	let config;
	let inputs;
	try {
		config = JSON.parse(configFile.bytes);
		inputs = JSON.parse(inputsFile.bytes);
	} catch (error) {
		throw new Error(`release provenance JSON is invalid: ${error.message}`, {
			cause: error,
		});
	}
	if (
		!Array.isArray(provenance.references) ||
		provenance.references.length === 0
	) {
		throw new Error('release provenance must contain reference inputs');
	}
	const references = [];
	for (const [index, reference] of provenance.references.entries()) {
		const file = await verifyPromotedFile(
			releaseRoot,
			reference,
			`reference[${index}]`,
		);
		validatePng(file.bytes, null, `reference[${index}]`, false);
		references.push({ record: reference, file });
	}
	return { config, configFile, inputs, inputsFile, references };
}

async function verifyEntryFiles(entry, releaseRoot, label, shared) {
	const generation = assertObject(entry.generation, `${label}.generation`);
	const receiptFile = await readVerifiedPath(
		releaseRoot,
		generation.receipt_path,
		generation.receipt_sha256,
		`${label}.receipt`,
	);
	let receipt;
	try {
		receipt = JSON.parse(receiptFile.bytes);
	} catch (error) {
		throw new Error(`${label}.receipt is not valid JSON: ${error.message}`, {
			cause: error,
		});
	}
	if (
		receipt.schema_version !== 1 ||
		receipt.receipt_type !== 'notebook-person-art-pair-candidate' ||
		receipt.status !== 'candidate' ||
		receipt.review?.status !== 'pending' ||
		receipt.promotion?.eligible !== false ||
		receipt.asset?.kind !== 'pair' ||
		canonicalJson(receipt.asset.children) !==
			canonicalJson(['portrait', 'marker'])
	) {
		throw new Error(`${label}.receipt is not a pending v1 pair receipt`);
	}
	if (
		receipt.job_id !== entry.job_id ||
		canonicalJson(receipt.subject) !== canonicalJson(entry.subject) ||
		receipt.asset.candidate_index !== entry.candidate_index ||
		canonicalJson(receipt.provider) !== canonicalJson(entry.provider)
	) {
		throw new Error(`${label}.receipt identity does not match release entry`);
	}
	const prompt = await readVerifiedPath(
		releaseRoot,
		generation.prompt_path,
		generation.prompt_file_sha256,
		`${label}.prompt`,
	);
	if (
		promptPayloadHash(prompt.bytes) !==
		assertSha256(generation.prompt_sha256, `${label}.prompt_sha256`)
	) {
		throw new Error(`${label}.prompt payload hash mismatch`);
	}
	const inputRecord = await readVerifiedPath(
		releaseRoot,
		generation.input_record_path,
		generation.input_record_file_sha256,
		`${label}.input record`,
	);
	let inputRecordValue;
	try {
		inputRecordValue = JSON.parse(inputRecord.bytes);
	} catch (error) {
		throw new Error(
			`${label}.input record is not valid JSON: ${error.message}`,
			{ cause: error },
		);
	}
	const inputRecordHash = sha256(canonicalJson(inputRecordValue));
	if (
		inputRecordHash !==
		assertSha256(generation.input_record_sha256, `${label}.input_record_sha256`)
	) {
		throw new Error(`${label}.input record canonical hash mismatch`);
	}
	if (entry.subject.input_record_sha256 !== inputRecordHash) {
		throw new Error(`${label}.subject input record hash mismatch`);
	}
	if (
		receipt.subject.input_record_sha256 !== inputRecordHash ||
		receipt.provenance?.config_sha256 !== shared.configFile.sha256 ||
		receipt.provenance?.inputs_sha256 !== shared.inputsFile.sha256 ||
		receipt.provenance?.prompt_sha256 !== generation.prompt_sha256
	) {
		throw new Error(`${label}.receipt generation provenance does not match`);
	}
	const sourceRecord =
		receipt.subject.kind === 'fallback'
			? shared.inputs.fallback
			: shared.inputs.npcs?.find(
					(npc) => npc.npc_id === receipt.subject.npc_id,
				);
	if (canonicalJson(sourceRecord) !== canonicalJson(inputRecordValue)) {
		throw new Error(`${label}.input record does not match the release dataset`);
	}
	const configuredProvider = shared.config.provider ?? {};
	if (
		canonicalJson({
			id: receipt.provider?.id,
			adapter: receipt.provider?.adapter,
			model: receipt.provider?.model,
			endpoint: receipt.provider?.endpoint,
			request: receipt.provider?.request,
		}) !==
		canonicalJson({
			id: configuredProvider.id,
			adapter: configuredProvider.adapter,
			model: configuredProvider.model,
			endpoint: configuredProvider.endpoint,
			request: configuredProvider.request,
		})
	) {
		throw new Error(
			`${label}.receipt provider does not match generation config`,
		);
	}
	const receiptReferences = receipt.provenance?.reference_inputs ?? [];
	const releaseReferences = shared.references.map(({ record }) => ({
		id: record.id,
		purpose: record.purpose,
		sha256: record.sha256,
	}));
	if (
		canonicalJson(
			receiptReferences.map(({ id, purpose, sha256: hash }) => ({
				id,
				purpose,
				sha256: hash,
			})),
		) !== canonicalJson(releaseReferences)
	) {
		throw new Error(`${label}.receipt references do not match release copies`);
	}
	const configuredReferences = (shared.config.reference_inputs ?? [])
		.filter(
			(reference) =>
				!reference.asset_kinds || reference.asset_kinds.includes('pair'),
		)
		.map(({ id, path, purpose }) => ({ id, path, purpose }));
	if (
		canonicalJson(
			receiptReferences.map(({ id, path, purpose }) => ({ id, path, purpose })),
		) !== canonicalJson(configuredReferences)
	) {
		throw new Error(
			`${label}.receipt references do not match generation config`,
		);
	}
	const rawArtifact = assertObject(
		generation.raw_artifact,
		`${label}.generation.raw_artifact`,
	);
	const providerRaw = await readVerifiedPath(
		releaseRoot,
		rawArtifact.path,
		rawArtifact.sha256,
		`${label}.provider raw`,
	);
	if (receipt.artifact?.raw_sha256 !== rawArtifact.sha256) {
		throw new Error(`${label}.provider raw hash does not match receipt`);
	}
	validatePng(
		providerRaw.bytes,
		dimensions(receipt.artifact, `${label}.receipt artifact`),
		`${label}.provider raw`,
		false,
	);

	const approval = assertObject(entry.approval, `${label}.approval`);
	if (
		approval.decision !== 'approved' ||
		approval.promotion_eligible !== true
	) {
		throw new Error(`${label} is not approved and promotion eligible`);
	}
	assertExactChecklist(
		approval.checklist,
		REQUIRED_PAIR_REVIEW_CHECKS,
		`${label}.manifest pair checklist`,
	);
	const decisionFile = await readVerifiedPath(
		releaseRoot,
		approval.decision_path,
		approval.decision_sha256,
		`${label}.decision`,
	);
	let decision;
	try {
		decision = JSON.parse(decisionFile.bytes);
	} catch (error) {
		throw new Error(`${label}.decision is not valid JSON: ${error.message}`, {
			cause: error,
		});
	}
	assertExactObjectKeys(
		decision,
		[
			'schema_version',
			'record_type',
			'candidate_receipt_path',
			'candidate_receipt_sha256',
			'candidate_sha256',
			'raw_sha256',
			'subject',
			'asset',
			'hair_topology',
			'marker_identity',
			'decision',
			'promotion_eligible',
			'reviewer',
			'reviewed_at',
			'notes',
			'checklist',
			'source_template_path',
			'review_id',
		],
		`${label}.decision`,
	);
	assertExactChecklist(
		decision.checklist,
		REQUIRED_PAIR_REVIEW_CHECKS,
		`${label}.decision pair checklist`,
	);
	assertCanonicalHairTopologyBinding(
		decision.hair_topology,
		entry.subject,
		`${label}.decision hair topology`,
	);
	assertCanonicalMarkerIdentityBinding(
		decision.marker_identity,
		entry.subject,
		`${label}.decision marker identity`,
	);
	if (
		decision.schema_version !== 1 ||
		decision.record_type !== 'notebook-person-art-human-review-decision' ||
		decision.decision !== 'approved' ||
		decision.promotion_eligible !== true ||
		decision.review_id !== computeReviewId(decision) ||
		decision.review_id !== approval.review_id ||
		decision.candidate_receipt_sha256 !== generation.receipt_sha256 ||
		decision.candidate_sha256 !== pairCandidateDigest(receipt) ||
		decision.raw_sha256 !== providerRaw.sha256 ||
		canonicalJson(decision.subject) !== canonicalJson(entry.subject) ||
		canonicalJson(decision.asset) !== canonicalJson(receipt.asset) ||
		decision.reviewer !== approval.reviewer ||
		decision.reviewed_at !== approval.reviewed_at ||
		decision.notes !== approval.notes ||
		approval.hair_topology_sha256 !== decision.hair_topology.sha256 ||
		approval.marker_identity_sha256 !== decision.marker_identity.sha256 ||
		canonicalJson(decision.checklist) !== canonicalJson(approval.checklist)
	) {
		throw new Error(`${label}.decision does not match release entry`);
	}
	const identity = {
		schema_version: 1,
		pipeline_revision: shared.config.pipeline_revision,
		provider: {
			id: configuredProvider.id,
			adapter: configuredProvider.adapter,
			model: configuredProvider.model,
			request: configuredProvider.request,
		},
		raw_output: shared.config.raw_output,
		validation: shared.config.validation,
		reference_inputs: receiptReferences.map(({ id, sha256: hash }) => ({
			id,
			sha256: hash,
		})),
		subject_kind: receipt.subject.kind,
		npc_id: receipt.subject.npc_id,
		input_record_sha256: inputRecordHash,
		asset_kind: 'pair',
		candidate_index: receipt.asset.candidate_index,
		prompt_sha256: generation.prompt_sha256,
	};
	if (sha256(canonicalJson(identity)) !== receipt.job_id) {
		throw new Error(
			`${label}.receipt job identity does not match copied inputs`,
		);
	}
	return { receipt, receiptFile, decision, decisionFile, providerRaw };
}

async function loadArt(entry, kind, releaseRoot, receipt, label) {
	const art = assertObject(entry.art?.[kind], `${label}.${kind}`);
	if (art.media_type !== 'image/png')
		throw new Error(`${label}.${kind} media_type must be image/png`);
	const expectedDimensions = dimensions(art, `${label}.${kind}`);
	const master = await readVerifiedPath(
		releaseRoot,
		art.master_path,
		art.sha256,
		`${label}.${kind}.master`,
	);
	const raw = await readVerifiedPath(
		releaseRoot,
		art.raw_path,
		art.source_raw_sha256,
		`${label}.${kind}.raw`,
	);
	assertString(
		art.source_candidate_path,
		`${label}.${kind}.source candidate path`,
	);
	assertString(art.source_raw_path, `${label}.${kind}.source raw path`);
	assertSha256(art.source_raw_sha256, `${label}.${kind}.source raw hash`);
	const masterImage = validatePng(
		master.bytes,
		expectedDimensions,
		`${label}.${kind}.master`,
		true,
	);
	validatePng(raw.bytes, expectedDimensions, `${label}.${kind}.raw`, false);
	const receiptChild = assertObject(
		receipt.artifact?.children?.[kind],
		`${label}.receipt ${kind}`,
	);
	if (
		receiptChild.candidate_sha256 !== art.sha256 ||
		receiptChild.raw_sha256 !== art.source_raw_sha256 ||
		receiptChild.width !== art.width ||
		receiptChild.height !== art.height
	) {
		throw new Error(`${label}.${kind} provenance does not match receipt`);
	}
	return { art, master, masterImage, raw };
}

async function verifyWholeCastApproval(manifest, releaseRoot, loadedEntries) {
	const approval = assertObject(manifest.approval, 'release approval');
	const descriptor = assertObject(
		approval.whole_cast_visual_review,
		'release whole-cast visual review',
	);
	const recordFile = await readVerifiedPath(
		releaseRoot,
		descriptor.path,
		descriptor.sha256,
		'whole-cast visual review',
	);
	let record;
	try {
		record = JSON.parse(recordFile.bytes);
	} catch (error) {
		throw new Error(`whole-cast review is not valid JSON: ${error.message}`, {
			cause: error,
		});
	}
	assertExactObjectKeys(
		record,
		[
			'schema_version',
			'record_type',
			'source_packets',
			'cast',
			'decision',
			'promotion_eligible',
			'reviewer',
			'reviewed_at',
			'notes',
			'checklist',
			'source_template_path',
			'review_id',
		],
		'whole-cast review',
	);
	assertExactChecklist(
		record.checklist,
		REQUIRED_WHOLE_CAST_REVIEW_CHECKS,
		'whole-cast review checklist',
	);
	if (
		record.schema_version !== 1 ||
		record.record_type !==
			'notebook-person-art-whole-cast-human-review-decision' ||
		record.decision !== 'approved' ||
		record.promotion_eligible !== true ||
		!record.reviewer ||
		!record.reviewed_at ||
		record.review_id !== computeReviewId(record) ||
		record.review_id !== descriptor.review_id
	) {
		throw new Error('whole-cast review identity or approval is invalid');
	}
	if (
		!Array.isArray(record.source_packets) ||
		record.source_packets.length === 0 ||
		record.source_packets.length > 3 ||
		record.source_packets.some(
			(packet) => !packet?.path || !/^[a-f0-9]{64}$/.test(packet.sha256 ?? ''),
		)
	) {
		throw new Error('whole-cast review packet provenance is invalid');
	}
	assertExactObjectKeys(
		record.cast,
		['named_count', 'total_count', 'members'],
		'whole-cast binding',
	);
	if (
		record.cast.named_count !== loadedEntries.length - 1 ||
		record.cast.total_count !== loadedEntries.length ||
		!Array.isArray(record.cast.members) ||
		record.cast.members.length !== loadedEntries.length
	) {
		throw new Error('whole-cast review does not bind the complete release');
	}
	const seen = new Set();
	for (const [index, loaded] of loadedEntries.entries()) {
		const member = record.cast.members[index];
		assertExactObjectKeys(
			member,
			[
				'subject_key',
				'subject',
				'candidate_receipt_path',
				'candidate_receipt_sha256',
				'candidate_sha256',
				'raw_sha256',
				'portrait_sha256',
				'marker_sha256',
				'hair_topology',
				'marker_identity',
			],
			`whole-cast member[${index}]`,
		);
		if (seen.has(member.subject_key)) {
			throw new Error('whole-cast review contains duplicate subjects');
		}
		seen.add(member.subject_key);
		assertString(
			member.candidate_receipt_path,
			`whole-cast member[${index}].candidate receipt path`,
		);
		if (
			member.subject_key !== subjectKey(loaded.entry.subject) ||
			member.candidate_receipt_sha256 !== loaded.verified.receiptFile.sha256 ||
			member.candidate_sha256 !==
				pairCandidateDigest(loaded.verified.receipt) ||
			member.raw_sha256 !== loaded.verified.providerRaw.sha256 ||
			member.portrait_sha256 !== loaded.portrait.master.sha256 ||
			member.marker_sha256 !== loaded.marker.master.sha256 ||
			canonicalJson(member.hair_topology) !==
				canonicalJson(loaded.verified.decision.hair_topology) ||
			canonicalJson(member.marker_identity) !==
				canonicalJson(loaded.verified.decision.marker_identity) ||
			canonicalJson(member.subject) !== canonicalJson(loaded.entry.subject)
		) {
			throw new Error(
				`whole-cast member ${member.subject_key ?? index} does not match release bytes`,
			);
		}
	}
	return { record, recordFile };
}

function validateReleaseManifest(manifest, expectedNamedCount, allowFixture) {
	assertObject(manifest, 'approved release manifest');
	if (
		manifest.schema_version !== 1 ||
		manifest.manifest_type !== RELEASE_MANIFEST_TYPE
	) {
		throw new Error(
			`approved release manifest must be v1 ${RELEASE_MANIFEST_TYPE}`,
		);
	}
	if (manifest.mode !== 'production' && manifest.mode !== 'fixture') {
		throw new Error(
			'approved release manifest mode must be production or fixture',
		);
	}
	if (manifest.mode === 'fixture' && !allowFixture) {
		throw new Error('fixture release manifests require allowFixture: true');
	}
	if (
		!Array.isArray(manifest.entries) ||
		manifest.entry_count !== manifest.entries.length
	) {
		throw new Error(
			'approved release manifest entry_count does not match entries',
		);
	}
	const { release_id: _releaseId, ...manifestBase } = manifest;
	const expectedReleaseId = sha256(canonicalJson(manifestBase));
	if (
		assertSha256(
			manifest.release_id,
			'approved release manifest.release_id',
		) !== expectedReleaseId
	) {
		throw new Error(
			`approved release manifest release_id mismatch: expected ${expectedReleaseId}, got ${manifest.release_id}`,
		);
	}
	const named = manifest.entries.filter(
		(entry) => entry.subject?.kind === 'npc',
	);
	const fallback = manifest.entries.filter(
		(entry) => entry.subject?.kind === 'fallback',
	);
	if (named.length !== expectedNamedCount || fallback.length !== 1) {
		throw new Error(
			`approved release manifest must contain exactly ${expectedNamedCount} named people plus one fallback; got ${named.length} named and ${fallback.length} fallback`,
		);
	}
	const npcIds = named.map((entry, index) =>
		positiveInteger(
			entry.subject?.npc_id,
			`named entry[${index}].subject.npc_id`,
		),
	);
	if (new Set(npcIds).size !== npcIds.length)
		throw new Error(
			'approved release manifest contains duplicate npc_id values',
		);
	if (npcIds.some((npcId, index) => index > 0 && npcId <= npcIds[index - 1])) {
		throw new Error(
			'approved release manifest NPC entries must be sorted by npc_id',
		);
	}
	if (manifest.entries.at(-1) !== fallback[0]) {
		throw new Error(
			'approved release manifest fallback must be the final entry',
		);
	}
	if (fallback[0].subject?.npc_id !== null)
		throw new Error('approved fallback npc_id must be null');
	return { named, fallback: fallback[0] };
}

function runtimeRecord(loaded, manifestPath, manifestHash) {
	const { entry, portrait, marker, runtime } = loaded;
	return {
		npc_id: entry.subject.npc_id,
		real_name: entry.subject.name,
		display_name: entry.subject.name,
		portrait: runtime.portrait,
		marker: runtime.marker,
		approval_status: 'approved',
		provenance: {
			release_manifest: manifestPath,
			release_manifest_sha256: manifestHash,
			release_id: loaded.releaseId,
			job_id: entry.job_id,
			input_record_sha256: entry.subject.input_record_sha256,
			review_id: entry.approval.review_id,
			portrait: {
				master_path: portablePath(portrait.art.master_path),
				master_sha256: portrait.master.sha256,
				source_candidate_path: portablePath(portrait.art.source_candidate_path),
				source_raw_path: portablePath(portrait.art.source_raw_path),
				source_raw_sha256: portrait.art.source_raw_sha256,
			},
			marker: {
				master_path: portablePath(marker.art.master_path),
				master_sha256: marker.master.sha256,
				source_candidate_path: portablePath(marker.art.source_candidate_path),
				source_raw_path: portablePath(marker.art.source_raw_path),
				source_raw_sha256: marker.art.source_raw_sha256,
			},
		},
	};
}

function drawContactSheet(assets) {
	const cellWidth = 276;
	const cellHeight = 250;
	const padding = 24;
	const rows = Math.ceil(assets.length / CONTACT_COLUMNS);
	const sheet = createImage(
		CONTACT_COLUMNS * cellWidth + padding * 2,
		rows * cellHeight + padding * 2,
		[237, 218, 176, 255],
	);
	for (let row = 0; row <= rows; row += 1) {
		fillRect(
			sheet,
			padding,
			padding + row * cellHeight,
			CONTACT_COLUMNS * cellWidth,
			2,
			[87, 62, 38, 255],
		);
	}
	for (let column = 0; column <= CONTACT_COLUMNS; column += 1) {
		fillRect(
			sheet,
			padding + column * cellWidth,
			padding,
			2,
			rows * cellHeight,
			[87, 62, 38, 255],
		);
	}
	assets.forEach((asset, index) => {
		const left = padding + (index % CONTACT_COLUMNS) * cellWidth;
		const top = padding + Math.floor(index / CONTACT_COLUMNS) * cellHeight;
		compositeOpaque(
			sheet,
			resizeContain(asset.portrait, 126, 134),
			left + 24,
			top + 42,
		);
		compositeOpaque(
			sheet,
			resizeContain(asset.marker, 96, 136),
			left + 158,
			top + 40,
		);
	});
	return sheet;
}

function escapeHtml(value) {
	return String(value)
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;');
}

function contactSheetHtml(releaseId, records) {
	const figures = records
		.map(
			(person) => `<figure>
	<img src="${escapeHtml(person.portrait)}" alt="${escapeHtml(person.display_name)} portrait">
	<img src="${escapeHtml(person.marker)}" alt="${escapeHtml(person.display_name)} marker">
	<figcaption>${escapeHtml(person.display_name)}${person.npc_id === null ? ' (fallback)' : ` (NPC ${person.npc_id})`}</figcaption>
</figure>`,
		)
		.join('\n');
	return `<!doctype html>
<meta charset="utf-8">
<title>Rundale Notebook Person Art Contact Sheet</title>
<style>
body { margin: 24px; background: #ead8af; color: #2f2316; font-family: Georgia, serif; }
h1 { font-size: 24px; font-weight: 400; }
.sheet { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 18px; max-width: 1100px; }
figure { margin: 0; padding: 14px; border: 1px solid rgba(47, 35, 22, 0.35); background: rgba(255, 248, 222, 0.45); }
img { width: 46%; height: 140px; object-fit: contain; vertical-align: middle; }
figcaption { margin-top: 8px; font-size: 15px; }
</style>
<h1>Rundale Notebook Person Art Contact Sheet</h1>
<p>Approved release: ${escapeHtml(releaseId)}; ${records.length - 1} named people plus approved fallback.</p>
<div class="sheet">
${figures}
</div>
`;
}

function provenanceMarkdown(releaseId, records, releasePath, releaseHash) {
	const rows = records
		.map(
			(person) =>
				`| ${person.npc_id ?? 'fallback'} | ${person.display_name} | ${person.portrait} | ${person.marker} | ${person.provenance.portrait.master_sha256} | ${person.provenance.marker.master_sha256} |`,
		)
		.join('\n');
	return `# Notebook Person Art Provenance

Generated deterministically by \`parish/apps/ui/scripts/build-notebook-person-art.mjs\`.
The sole shipping authority is approved release \`${releaseId}\` at
\`${releasePath}\` (manifest SHA-256 \`${releaseHash}\`). Legacy source sheets are
not an approval authority and are not consumed by this builder.

The builder verifies the release ID and every promoted/source hash referenced by
the release manifest before writing output. PNG masters and source candidates are
dimension-checked and must contain both visible pixels and transparent background.

| NPC ID | Person | Portrait | Marker | Portrait master SHA-256 | Marker master SHA-256 |
| --- | --- | --- | --- | --- | --- |
${rows}

Review artifacts: \`person-art-contact-sheet.png\` and
\`person-art-contact-sheet.html\` (${CONTACT_COLUMNS} columns,
${Math.ceil(records.length / CONTACT_COLUMNS)} rows).
`;
}

function assetReadme(releasePath) {
	return `# Rundale Illustrated Notebook Runtime UI Assets

This directory is the production runtime asset kit for the illustrated notebook.

Approved portraits and complete in-world markers are built from the deterministic
approved-release manifest at \`${releasePath}\`. The builder verifies the release,
master, source candidate, raw source, generation, and approval hashes plus PNG
dimensions and content. Portraits and markers are contain-scaled on transparent
canvases, so complete figures are never cropped.

\`asset-manifest.json\` records NPC IDs and per-asset provenance.
\`person-art-provenance.md\` records the approved release and master hashes.
\`person-art-contact-sheet.png\` and \`person-art-contact-sheet.html\` show all
named pairs and the approved fallback in a dynamic four-column grid.
`;
}

async function writeRuntimeImage(runtimeRoot, configuredPath, image, master) {
	const target = containedPath(
		runtimeRoot,
		configuredPath,
		'runtime asset path',
	);
	await mkdir(dirname(target), { recursive: true });
	const bytes =
		image.width === master.masterImage.width &&
		image.height === master.masterImage.height
			? master.master.bytes
			: encodePng(image);
	await writeFile(target, bytes);
}

export async function buildNotebookPersonArt(options = {}) {
	const uiRoot = resolve(options.uiRoot ?? defaultUiRoot);
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	const releaseManifestPath = resolve(
		options.releaseManifestPath ??
			join(
				uiRoot,
				'art',
				'notebook-person-art',
				'approved',
				'v1',
				'release-manifest.json',
			),
	);
	const releaseRoot = resolve(
		options.releaseRoot ?? dirname(releaseManifestPath),
	);
	const runtimeRoot = resolve(
		options.runtimeRoot ?? join(uiRoot, 'static', 'rundale', 'notebook-ui'),
	);
	const expectedNamedCount = options.expectedNamedCount ?? NAMED_CAST_SIZE;
	const runtimeSizes = {
		portrait: dimensions(
			options.runtimeSizes?.portrait ?? DEFAULT_RUNTIME_SIZES.portrait,
			'runtime portrait size',
		),
		marker: dimensions(
			options.runtimeSizes?.marker ?? DEFAULT_RUNTIME_SIZES.marker,
			'runtime marker size',
		),
	};
	const releaseBytes = await readFile(releaseManifestPath);
	const releaseHash = sha256(releaseBytes);
	const manifest = JSON.parse(releaseBytes);
	const releaseEntries = validateReleaseManifest(
		manifest,
		expectedNamedCount,
		options.allowFixture === true,
	);
	const shared = await verifyReleaseProvenance(manifest, releaseRoot);

	const orderedEntries = [...releaseEntries.named, releaseEntries.fallback];
	const loadedEntries = [];
	const runtimePaths = new Set();
	for (const [index, entry] of orderedEntries.entries()) {
		const label =
			entry.subject.kind === 'fallback'
				? 'fallback'
				: `NPC ${entry.subject.npc_id}`;
		assertString(entry.subject.name, `${label}.subject.name`);
		assertSha256(entry.job_id, `${label}.job_id`);
		assertSha256(
			entry.subject.input_record_sha256,
			`${label}.subject.input_record_sha256`,
		);
		const verified = await verifyEntryFiles(entry, releaseRoot, label, shared);
		const slug =
			entry.subject.kind === 'fallback'
				? 'unknown-neighbour'
				: slugify(entry.subject.name);
		const runtime = {
			portrait: `people/portrait-${slug}.png`,
			marker: `people/marker-${slug}.png`,
		};
		for (const path of Object.values(runtime)) {
			containedPath(runtimeRoot, path, `${label} runtime path`);
			if (runtimePaths.has(path))
				throw new Error(`duplicate runtime asset path: ${path}`);
			runtimePaths.add(path);
		}
		loadedEntries.push({
			entry,
			verified,
			runtime,
			releaseId: manifest.release_id,
			portrait: await loadArt(
				entry,
				'portrait',
				releaseRoot,
				verified.receipt,
				label,
			),
			marker: await loadArt(
				entry,
				'marker',
				releaseRoot,
				verified.receipt,
				label,
			),
			index,
		});
	}
	await verifyWholeCastApproval(manifest, releaseRoot, loadedEntries);

	let runtimeManifest = { version: 3, assets: {} };
	const baseManifestPath =
		options.baseManifestPath === null
			? null
			: resolve(
					options.baseManifestPath ?? join(runtimeRoot, 'asset-manifest.json'),
				);
	if (baseManifestPath) {
		runtimeManifest = JSON.parse(await readFile(baseManifestPath, 'utf8'));
	}

	// Build a complete sibling tree before replacing the shipping directory.
	const stagingRoot = join(
		dirname(runtimeRoot),
		`.${basename(runtimeRoot)}.tmp-${process.pid}-${randomUUID()}`,
	);
	await rm(stagingRoot, { recursive: true, force: true });
	await mkdir(dirname(runtimeRoot), { recursive: true });
	if (await pathExists(runtimeRoot)) {
		await cp(runtimeRoot, stagingRoot, { recursive: true });
	} else {
		await mkdir(stagingRoot, { recursive: true });
	}
	const records = [];
	const contactAssets = [];
	try {
		await rm(join(stagingRoot, 'people'), { recursive: true, force: true });
		const manifestLabel = portablePath(
			options.releaseManifestLabel ?? relative(repoRoot, releaseManifestPath),
		);
		for (const loaded of loadedEntries) {
			const portrait = resizeContain(
				loaded.portrait.masterImage,
				runtimeSizes.portrait.width,
				runtimeSizes.portrait.height,
			);
			const marker = resizeContain(
				loaded.marker.masterImage,
				runtimeSizes.marker.width,
				runtimeSizes.marker.height,
			);
			validatePng(
				encodePng(portrait),
				runtimeSizes.portrait,
				`${loaded.entry.subject.name}.runtime portrait`,
				true,
			);
			validatePng(
				encodePng(marker),
				runtimeSizes.marker,
				`${loaded.entry.subject.name}.runtime marker`,
				true,
			);
			await writeRuntimeImage(
				stagingRoot,
				loaded.runtime.portrait,
				portrait,
				loaded.portrait,
			);
			await writeRuntimeImage(
				stagingRoot,
				loaded.runtime.marker,
				marker,
				loaded.marker,
			);
			records.push(runtimeRecord(loaded, manifestLabel, releaseHash));
			contactAssets.push({ portrait, marker });
		}

		await writeFile(
			join(stagingRoot, 'person-art-contact-sheet.png'),
			encodePng(drawContactSheet(contactAssets)),
		);
		await writeFile(
			join(stagingRoot, 'person-art-contact-sheet.html'),
			contactSheetHtml(manifest.release_id, records),
		);
		runtimeManifest.version = Math.max(Number(runtimeManifest.version ?? 1), 3);
		runtimeManifest.assets ??= {};
		runtimeManifest.assets.portraits = records.map((person) => person.portrait);
		runtimeManifest.assets.npcMarkers = records.map((person) => person.marker);
		runtimeManifest.assets.personArt = {
			release_id: manifest.release_id,
			release_manifest: manifestLabel,
			release_manifest_sha256: releaseHash,
			approval_status: 'approved',
			contact_sheet: 'person-art-contact-sheet.png',
			contact_sheet_html: 'person-art-contact-sheet.html',
			fallback: records.at(-1),
			people: records.slice(0, -1),
		};
		await writeFile(
			join(stagingRoot, 'asset-manifest.json'),
			`${JSON.stringify(runtimeManifest, null, '\t')}\n`,
		);
		await writeFile(
			join(stagingRoot, 'person-art-provenance.md'),
			provenanceMarkdown(
				manifest.release_id,
				records,
				manifestLabel,
				releaseHash,
			),
		);
		await writeFile(
			join(stagingRoot, 'asset-readme.md'),
			assetReadme(manifestLabel),
		);
		await replaceRuntimeTree(stagingRoot, runtimeRoot);
	} catch (error) {
		await rm(stagingRoot, { recursive: true, force: true });
		throw error;
	}

	return {
		release_id: manifest.release_id,
		release_manifest_sha256: releaseHash,
		named_count: releaseEntries.named.length,
		fallback_count: 1,
		runtime_root: runtimeRoot,
		contact_sheet_rows: Math.ceil(records.length / CONTACT_COLUMNS),
	};
}

async function main() {
	const { values } = parseArgs({
		options: {
			'release-manifest': { type: 'string' },
			'release-root': { type: 'string' },
			'repo-root': { type: 'string' },
			'runtime-root': { type: 'string' },
			'base-manifest': { type: 'string' },
		},
	});
	const result = await buildNotebookPersonArt({
		releaseManifestPath: values['release-manifest'],
		releaseRoot: values['release-root'],
		repoRoot: values['repo-root'],
		runtimeRoot: values['runtime-root'],
		baseManifestPath: values['base-manifest'],
	});
	console.log(
		`Built ${result.named_count} approved named portrait/marker pairs plus fallback in ${result.runtime_root}`,
	);
}

const isMain =
	process.argv[1] &&
	pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (isMain) await main();
