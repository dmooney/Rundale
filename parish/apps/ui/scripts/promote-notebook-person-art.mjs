import { randomUUID } from 'node:crypto';
import { execFile } from 'node:child_process';
import {
	copyFile,
	mkdtemp,
	mkdir,
	readFile,
	rename,
	rm,
	stat,
	writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import {
	basename,
	dirname,
	isAbsolute,
	join,
	relative,
	resolve,
} from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { promisify } from 'node:util';
import {
	assertExactChecklist,
	assertHairTopologyBinding,
	assertMarkerIdentityBinding,
	canonicalJson,
	computeReviewId,
	NAMED_CAST_SIZE,
	pairCandidateDigest,
	REQUIRED_PAIR_REVIEW_CHECKS,
	REQUIRED_WHOLE_CAST_REVIEW_CHECKS,
	sha256,
	subjectKey as reviewSubjectKey,
} from './notebook-person-art-approval-contract.mjs';

export { REQUIRED_PAIR_REVIEW_CHECKS } from './notebook-person-art-approval-contract.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(here, '..');
const defaultRepoRoot = resolve(uiRoot, '../../..');
const defaultOutput = join(
	uiRoot,
	'art',
	'notebook-person-art',
	'approved',
	'v1',
);
const expectedProductionNpcCount = NAMED_CAST_SIZE;
const canonicalConfigPath =
	'parish/apps/ui/art/notebook-person-art/generation-config-v1.json';
const canonicalInputsPath =
	'parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json';
const canonicalArtDirectionPath =
	'parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json';
const canonicalNpcsPath = 'mods/rundale/npcs.json';
const canonicalWorldPath = 'mods/rundale/world.json';
const execFileAsync = promisify(execFile);

export class PromotionError extends Error {}

function prettyJson(value) {
	return `${JSON.stringify(value, null, '\t')}\n`;
}

function resolvePath(repoRoot, configuredPath) {
	if (typeof configuredPath !== 'string' || !configuredPath) {
		throw new PromotionError('Provenance path is missing');
	}
	return isAbsolute(configuredPath)
		? resolve(configuredPath)
		: resolve(repoRoot, configuredPath);
}

function portablePath(root, path) {
	const local = relative(root, path);
	return local && !local.startsWith('..') && !isAbsolute(local)
		? local.replaceAll('\\', '/')
		: path;
}

async function exists(path) {
	try {
		await stat(path);
		return true;
	} catch (error) {
		if (error.code === 'ENOENT') return false;
		throw error;
	}
}

async function loadFile(path, label) {
	let bytes;
	try {
		bytes = await readFile(path);
	} catch (error) {
		throw new PromotionError(
			`Could not read ${label} ${path}: ${error.message}`,
		);
	}
	return { path, bytes, sha256: sha256(bytes) };
}

async function loadJson(path, label) {
	const file = await loadFile(path, label);
	try {
		return { ...file, value: JSON.parse(file.bytes.toString('utf8')) };
	} catch (error) {
		throw new PromotionError(
			`Could not parse ${label} ${path}: ${error.message}`,
		);
	}
}

function assertHash(actual, expected, label) {
	if (typeof expected !== 'string' || actual !== expected) {
		throw new PromotionError(`${label} hash does not match`);
	}
}

function assertSame(left, right, label) {
	if (canonicalJson(left) !== canonicalJson(right)) {
		throw new PromotionError(`${label} does not match`);
	}
}

function assertExactObjectKeys(value, expected, label) {
	const actual = Object.keys(value ?? {}).toSorted();
	const wanted = [...expected].toSorted();
	if (canonicalJson(actual) !== canonicalJson(wanted)) {
		throw new PromotionError(`${label} keys do not match the contract`);
	}
}

function assertPng(file, label) {
	if (file.bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a') {
		throw new PromotionError(`${label} is not a PNG`);
	}
}

function releaseSubjectKey(subject) {
	if (subject?.kind === 'fallback' && subject.npc_id === null)
		return 'fallback';
	if (
		subject?.kind === 'npc' &&
		Number.isInteger(subject.npc_id) &&
		subject.npc_id > 0
	) {
		return String(subject.npc_id);
	}
	throw new PromotionError(
		'Every approved subject must be a numeric NPC id or the fallback',
	);
}

function promptPayloadHash(bytes) {
	if (bytes.length > 0 && bytes.at(-1) === 0x0a)
		return sha256(bytes.subarray(0, -1));
	return sha256(bytes);
}

function validateDecisionShape(decision) {
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
		'Pair review decision',
	);
	if (
		decision.schema_version !== 1 ||
		decision.record_type !== 'notebook-person-art-human-review-decision'
	) {
		throw new PromotionError(
			'Decision is not a v1 notebook person-art review record',
		);
	}
	if (
		decision.decision !== 'approved' ||
		decision.promotion_eligible !== true
	) {
		throw new PromotionError(
			`Review decision is ${decision.decision ?? 'pending'} and is not promotion eligible`,
		);
	}
	try {
		assertExactChecklist(
			decision.checklist,
			REQUIRED_PAIR_REVIEW_CHECKS,
			'Pair approval checklist',
		);
	} catch (error) {
		throw new PromotionError(error.message);
	}
	if (!decision.reviewer || !decision.reviewed_at || !decision.review_id) {
		throw new PromotionError(
			'Approved review decision is missing reviewer provenance',
		);
	}
	const expectedReviewId = computeReviewId(decision);
	if (decision.review_id !== expectedReviewId) {
		throw new PromotionError('Review decision id does not match its content');
	}
}

async function validateSharedSource(repoRoot, pathInput, expectedHash, label) {
	const path = resolvePath(repoRoot, pathInput);
	const file = await loadFile(path, label);
	assertHash(file.sha256, expectedHash, label);
	return file;
}

async function validatePair(
	repoRoot,
	receiptInput,
	decisionInput,
	artDirection,
) {
	const receiptPath = resolvePath(repoRoot, receiptInput);
	const decisionPath = resolvePath(repoRoot, decisionInput);
	const [receiptFile, decisionFile] = await Promise.all([
		loadJson(receiptPath, 'candidate receipt'),
		loadJson(decisionPath, 'review decision'),
	]);
	const receipt = receiptFile.value;
	const decision = decisionFile.value;

	if (
		receipt.schema_version !== 1 ||
		receipt.receipt_type !== 'notebook-person-art-pair-candidate' ||
		receipt.status !== 'candidate' ||
		receipt.review?.status !== 'pending' ||
		receipt.promotion?.eligible !== false ||
		receipt.asset?.kind !== 'pair'
	) {
		throw new PromotionError(
			`${receiptPath} is not a pending v1 pair candidate receipt`,
		);
	}
	if (
		canonicalJson(receipt.asset.children) !==
		canonicalJson(['portrait', 'marker'])
	) {
		throw new PromotionError(
			'Pair receipt must contain portrait and marker children',
		);
	}
	validateDecisionShape(decision);

	const pointerPath = join(dirname(receiptPath), 'review.json');
	const pointerFile = await loadJson(pointerPath, 'review pointer');
	const pointer = pointerFile.value;
	if (resolvePath(repoRoot, pointer.record_path) !== decisionPath) {
		throw new PromotionError(
			'Review pointer does not name the supplied decision record',
		);
	}
	assertHash(decisionFile.sha256, pointer.record_sha256, 'Review decision');
	if (pointer.decision !== 'approved' || pointer.promotion_eligible !== true) {
		throw new PromotionError(
			'Review pointer is not approved and promotion eligible',
		);
	}

	assertHash(
		receiptFile.sha256,
		decision.candidate_receipt_sha256,
		'Candidate receipt',
	);
	if (resolvePath(repoRoot, decision.candidate_receipt_path) !== receiptPath) {
		throw new PromotionError(
			'Review decision points to a different candidate receipt',
		);
	}
	const candidateSha256 = pairCandidateDigest(receipt);
	for (const [label, actual] of [
		['Review decision candidate', decision.candidate_sha256],
		['Review pointer candidate', pointer.candidate_sha256],
	]) {
		assertHash(actual, candidateSha256, label);
	}
	assertHash(
		decision.raw_sha256,
		receipt.artifact.raw_sha256,
		'Review decision raw artifact',
	);
	assertSame(decision.subject, receipt.subject, 'Review decision subject');
	assertSame(decision.asset, receipt.asset, 'Review decision asset');
	let hairTopology;
	let markerIdentity;
	try {
		hairTopology = assertHairTopologyBinding(
			decision.hair_topology,
			artDirection,
			receipt.subject,
			'Pair review decision hair topology',
		);
	} catch (error) {
		throw new PromotionError(error.message);
	}
	try {
		markerIdentity = assertMarkerIdentityBinding(
			decision.marker_identity,
			artDirection,
			receipt.subject,
			'Pair review decision marker identity',
		);
	} catch (error) {
		throw new PromotionError(error.message);
	}

	const provenance = receipt.provenance ?? {};
	const [configFile, promptFile, inputRecordFile, providerRaw] =
		await Promise.all([
			validateSharedSource(
				repoRoot,
				provenance.config_path,
				provenance.config_sha256,
				'Generation config',
			),
			loadFile(
				resolvePath(repoRoot, provenance.prompt_path),
				'provider prompt',
			),
			loadJson(
				resolvePath(repoRoot, provenance.input_record_path),
				'input record',
			),
			validateSharedSource(
				repoRoot,
				receipt.artifact.raw_path,
				receipt.artifact.raw_sha256,
				'Provider raw artifact',
			),
		]);
	assertHash(
		promptPayloadHash(promptFile.bytes),
		provenance.prompt_sha256,
		'Provider prompt',
	);
	const inputRecordSha256 = sha256(canonicalJson(inputRecordFile.value));
	assertHash(
		inputRecordSha256,
		receipt.subject.input_record_sha256,
		'Input record',
	);

	let config;
	try {
		config = JSON.parse(configFile.bytes.toString('utf8'));
	} catch (error) {
		throw new PromotionError(
			`Could not parse generation config provenance: ${error.message}`,
		);
	}
	assertSame(
		{
			id: receipt.provider?.id,
			adapter: receipt.provider?.adapter,
			model: receipt.provider?.model,
			endpoint: receipt.provider?.endpoint,
			request: receipt.provider?.request,
		},
		{
			id: config.provider?.id,
			adapter: config.provider?.adapter,
			model: config.provider?.model,
			endpoint: config.provider?.endpoint,
			request: config.provider?.request,
		},
		'Receipt provider and generation config',
	);

	const references = [];
	for (const reference of provenance.reference_inputs ?? []) {
		const file = await validateSharedSource(
			repoRoot,
			reference.path,
			reference.sha256,
			`Reference ${reference.id}`,
		);
		assertPng(file, `Reference ${reference.id}`);
		references.push({ ...reference, file });
	}
	if (references.length === 0) {
		throw new PromotionError('Candidate receipt has no reference provenance');
	}
	const configuredReferences = (config.reference_inputs ?? [])
		.filter(
			(reference) =>
				!reference.asset_kinds || reference.asset_kinds.includes('pair'),
		)
		.map(({ id, path, purpose }) => ({ id, path, purpose }));
	assertSame(
		references.map(({ id, path, purpose }) => ({ id, path, purpose })),
		configuredReferences,
		'Receipt references and generation config',
	);

	const art = {};
	for (const kind of ['portrait', 'marker']) {
		const child = receipt.artifact.children?.[kind];
		if (!child?.raw_path || !child?.candidate_path) {
			throw new PromotionError(`Pair receipt is missing ${kind} artifacts`);
		}
		const [raw, candidate] = await Promise.all([
			validateSharedSource(
				repoRoot,
				child.raw_path,
				child.raw_sha256,
				`${kind} raw artifact`,
			),
			validateSharedSource(
				repoRoot,
				child.candidate_path,
				child.candidate_sha256,
				`${kind} candidate`,
			),
		]);
		assertPng(raw, `${kind} raw artifact`);
		assertPng(candidate, `${kind} candidate`);
		art[kind] = { receipt: child, raw, candidate };
	}
	assertPng(providerRaw, 'Provider raw artifact');

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
		reference_inputs: references.map(({ id, sha256: hash }) => ({
			id,
			sha256: hash,
		})),
		subject_kind: receipt.subject.kind,
		npc_id: receipt.subject.npc_id,
		input_record_sha256: inputRecordSha256,
		asset_kind: 'pair',
		candidate_index: receipt.asset.candidate_index,
		prompt_sha256: provenance.prompt_sha256,
	};
	assertHash(
		sha256(canonicalJson(identity)),
		receipt.job_id,
		'Candidate job id',
	);

	return {
		repoRoot,
		receipt,
		receiptFile,
		decision,
		decisionFile,
		pointerFile,
		configFile,
		promptFile,
		inputRecordFile,
		providerRaw,
		references,
		art,
		candidateSha256,
		hairTopology,
		markerIdentity,
		key: releaseSubjectKey(receipt.subject),
	};
}

function assertUnique(entries, selector, label) {
	const seen = new Set();
	for (const entry of entries) {
		const value = selector(entry);
		if (seen.has(value))
			throw new PromotionError(`Duplicate ${label}: ${value}`);
		seen.add(value);
	}
}

function currentInputRecord(inputs, subject) {
	return subject.kind === 'fallback'
		? inputs.fallback
		: inputs.npcs?.find((npc) => npc.npc_id === subject.npc_id);
}

function validateRelease(entries, mode, currentInputsFile) {
	assertUnique(entries, (entry) => entry.key, 'approved subject');
	assertUnique(entries, (entry) => entry.receipt.job_id, 'candidate job id');
	assertUnique(
		entries,
		(entry) => entry.receiptFile.path,
		'candidate receipt path',
	);
	assertUnique(
		entries,
		(entry) => entry.decisionFile.path,
		'review decision path',
	);
	assertUnique(
		entries,
		(entry) => entry.candidateSha256,
		'portrait-marker pair',
	);
	assertUnique(
		entries,
		(entry) => entry.art.portrait.candidate.sha256,
		'portrait master',
	);
	assertUnique(
		entries,
		(entry) => entry.art.marker.candidate.sha256,
		'marker master',
	);

	const first = entries[0];
	for (const entry of entries.slice(1)) {
		assertHash(
			entry.configFile.sha256,
			first.configFile.sha256,
			'Release config',
		);
		assertSame(
			entry.references.map(({ id, sha256: hash }) => ({ id, sha256: hash })),
			first.references.map(({ id, sha256: hash }) => ({ id, sha256: hash })),
			'Release references',
		);
	}
	const currentInputs = currentInputsFile.value;
	for (const entry of entries) {
		const sourceRecord = currentInputRecord(
			currentInputs,
			entry.receipt.subject,
		);
		if (!sourceRecord) {
			throw new PromotionError(
				`Current NPC art inputs are missing subject ${releaseSubjectKey(entry.receipt.subject)}`,
			);
		}
		assertSame(
			entry.inputRecordFile.value,
			sourceRecord,
			'Input record and current catalog entry',
		);
		assertHash(
			sha256(canonicalJson(sourceRecord)),
			entry.receipt.subject.input_record_sha256,
			'Current catalog input record',
		);
	}

	if (mode === 'production') {
		const npcIds = entries
			.filter((entry) => entry.receipt.subject.kind === 'npc')
			.map((entry) => entry.receipt.subject.npc_id)
			.toSorted((left, right) => left - right);
		const fallbackCount = entries.filter(
			(entry) => entry.key === 'fallback',
		).length;
		if (
			npcIds.length !== expectedProductionNpcCount ||
			npcIds.some((id) => !Number.isInteger(id) || id <= 0) ||
			fallbackCount !== 1
		) {
			throw new PromotionError(
				`Production promotion requires ${expectedProductionNpcCount} numeric NPC ids plus one fallback`,
			);
		}
		const expectedIds = (currentInputs.npcs ?? [])
			.map((npc) => npc.npc_id)
			.toSorted((left, right) => left - right);
		if (
			expectedIds.length !== expectedProductionNpcCount ||
			expectedIds.some((id) => !Number.isInteger(id)) ||
			canonicalJson(npcIds) !== canonicalJson(expectedIds) ||
			!currentInputs.fallback
		) {
			throw new PromotionError(
				'Production approvals do not exactly cover the 23 NPC input records and fallback',
			);
		}
		const canonicalInputs = resolve(currentInputsFile.path);
		for (const entry of entries) {
			if (
				resolvePath(entry.repoRoot, entry.receipt.provenance?.inputs_path) !==
				canonicalInputs
			) {
				throw new PromotionError(
					'Production candidates must name the repository canonical NPC art inputs catalog',
				);
			}
			assertHash(
				currentInputsFile.sha256,
				entry.receipt.provenance?.inputs_sha256,
				'Production candidate canonical NPC art inputs',
			);
		}
	}
}

async function validateWholeCastReview(repoRoot, pathInput, entries, mode) {
	if (!pathInput) {
		throw new PromotionError(
			'Promotion requires one immutable --cast-review decision',
		);
	}
	const recordPath = resolvePath(repoRoot, pathInput);
	const recordFile = await loadJson(recordPath, 'whole-cast review decision');
	const record = recordFile.value;
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
		'Whole-cast review decision',
	);
	if (
		record.schema_version !== 1 ||
		record.record_type !==
			'notebook-person-art-whole-cast-human-review-decision' ||
		record.decision !== 'approved' ||
		record.promotion_eligible !== true
	) {
		throw new PromotionError(
			'Whole-cast review is not an approved v1 promotion decision',
		);
	}
	if (!record.reviewer || !record.reviewed_at) {
		throw new PromotionError(
			'Whole-cast review is missing reviewer provenance',
		);
	}
	try {
		assertExactChecklist(
			record.checklist,
			REQUIRED_WHOLE_CAST_REVIEW_CHECKS,
			'Whole-cast approval checklist',
		);
	} catch (error) {
		throw new PromotionError(error.message);
	}
	if (record.review_id !== computeReviewId(record)) {
		throw new PromotionError('Whole-cast review id does not match its content');
	}
	if (
		!Array.isArray(record.source_packets) ||
		record.source_packets.length === 0 ||
		record.source_packets.length > 3
	) {
		throw new PromotionError(
			'Whole-cast review must bind one to three source packets',
		);
	}
	for (const packet of record.source_packets) {
		if (!packet?.path || !/^[a-f0-9]{64}$/.test(packet.sha256 ?? '')) {
			throw new PromotionError(
				'Whole-cast source packet provenance is invalid',
			);
		}
	}
	assertExactObjectKeys(
		record.cast,
		['named_count', 'total_count', 'members'],
		'Whole-cast binding',
	);
	if (!Array.isArray(record.cast.members)) {
		throw new PromotionError('Whole-cast binding members must be an array');
	}
	const sortedEntries = entries.toSorted((left, right) => {
		if (left.key === 'fallback') return 1;
		if (right.key === 'fallback') return -1;
		return Number(left.key) - Number(right.key);
	});
	const expectedNamedCount = sortedEntries.filter(
		(entry) => entry.receipt.subject.kind === 'npc',
	).length;
	if (
		record.cast.named_count !== expectedNamedCount ||
		record.cast.total_count !== sortedEntries.length ||
		record.cast.members.length !== sortedEntries.length ||
		(mode === 'production' && expectedNamedCount !== NAMED_CAST_SIZE)
	) {
		throw new PromotionError(
			'Whole-cast review does not bind the complete promotion set',
		);
	}
	const seen = new Set();
	for (const [index, entry] of sortedEntries.entries()) {
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
			`Whole-cast member[${index}]`,
		);
		const key = reviewSubjectKey(entry.receipt.subject);
		if (seen.has(member.subject_key)) {
			throw new PromotionError('Whole-cast review contains duplicate subjects');
		}
		seen.add(member.subject_key);
		if (
			member.subject_key !== key ||
			resolvePath(repoRoot, member.candidate_receipt_path) !==
				entry.receiptFile.path ||
			member.candidate_receipt_sha256 !== entry.receiptFile.sha256 ||
			member.candidate_sha256 !== entry.candidateSha256 ||
			member.raw_sha256 !== entry.providerRaw.sha256 ||
			member.portrait_sha256 !== entry.art.portrait.candidate.sha256 ||
			member.marker_sha256 !== entry.art.marker.candidate.sha256 ||
			canonicalJson(member.hair_topology) !==
				canonicalJson(entry.hairTopology) ||
			canonicalJson(member.marker_identity) !==
				canonicalJson(entry.markerIdentity) ||
			canonicalJson(member.subject) !== canonicalJson(entry.receipt.subject)
		) {
			throw new PromotionError(
				`Whole-cast member ${member.subject_key ?? index} does not match the approved candidate`,
			);
		}
	}
	return { record, recordFile };
}

async function validateCanonicalProductionConfig(entries, repoRoot) {
	const config = await loadFile(
		resolve(repoRoot, canonicalConfigPath),
		'canonical generation config',
	);
	for (const entry of entries) {
		assertHash(
			entry.configFile.sha256,
			config.sha256,
			'Production canonical generation config',
		);
	}
}

async function validateCanonicalProductionInputs(repoRoot, currentInputsFile) {
	const canonicalPath = resolve(repoRoot, canonicalInputsPath);
	if (resolve(currentInputsFile.path) !== canonicalPath) {
		throw new PromotionError(
			'Production promotion requires the repository canonical npc-art-inputs-v1.json',
		);
	}
	const sourcePaths = {
		npcs: resolve(repoRoot, canonicalNpcsPath),
		world: resolve(repoRoot, canonicalWorldPath),
		artDirection: resolve(repoRoot, canonicalArtDirectionPath),
	};
	await Promise.all([
		loadFile(sourcePaths.npcs, 'canonical NPC catalog'),
		loadFile(sourcePaths.world, 'canonical world'),
		loadFile(sourcePaths.artDirection, 'canonical NPC art direction'),
	]);
	const temporaryRoot = await mkdtemp(
		join(tmpdir(), 'rundale-art-input-freshness-'),
	);
	const regeneratedPath = join(temporaryRoot, 'npc-art-inputs-v1.json');
	try {
		await execFileAsync(
			'cargo',
			[
				'run',
				'--quiet',
				'--manifest-path',
				resolve(repoRoot, 'parish/Cargo.toml'),
				'-p',
				'parish-npc-tool',
				'--',
				'art-inputs',
				'--npcs',
				sourcePaths.npcs,
				'--world',
				sourcePaths.world,
				'--art-direction',
				sourcePaths.artDirection,
				'--output',
				regeneratedPath,
			],
			{
				cwd: repoRoot,
				maxBuffer: 8 * 1024 * 1024,
				timeout: 5 * 60 * 1000,
			},
		);
	} catch (error) {
		const detail = String(error.stderr ?? error.message).trim();
		await rm(temporaryRoot, { recursive: true, force: true });
		throw new PromotionError(
			`Could not prove canonical NPC art inputs freshness: ${detail}`,
		);
	}
	try {
		const regenerated = await loadFile(
			regeneratedPath,
			'regenerated canonical NPC art inputs',
		);
		if (!regenerated.bytes.equals(currentInputsFile.bytes)) {
			throw new PromotionError(
				'Canonical npc-art-inputs-v1.json is stale against canonical NPC, world, or art-direction sources; regenerate it with parish-npc-tool art-inputs',
			);
		}
	} finally {
		await rm(temporaryRoot, { recursive: true, force: true });
	}
}

async function copyExact(sourceFile, destination) {
	await mkdir(dirname(destination), { recursive: true });
	await copyFile(sourceFile.path, destination);
	const copied = await loadFile(destination, 'promoted file');
	assertHash(
		copied.sha256,
		sourceFile.sha256,
		`Promoted ${basename(destination)}`,
	);
	return copied;
}

function entryManifest(entry, releaseRoot) {
	const personRoot = join(releaseRoot, 'people', entry.key);
	const approved = (name) => portablePath(releaseRoot, join(personRoot, name));
	const childManifest = (kind) => ({
		master_path: approved(`${kind}.png`),
		sha256: entry.art[kind].candidate.sha256,
		raw_path: approved(`${kind}-raw.png`),
		media_type: entry.art[kind].receipt.media_type,
		width: entry.art[kind].receipt.width,
		height: entry.art[kind].receipt.height,
		source_candidate_path: portablePath(
			entry.repoRoot,
			entry.art[kind].candidate.path,
		),
		source_raw_path: portablePath(entry.repoRoot, entry.art[kind].raw.path),
		source_raw_sha256: entry.art[kind].raw.sha256,
		validation: {
			raw: entry.art[kind].receipt.raw_validation ?? null,
			candidate: entry.art[kind].receipt.candidate_validation ?? null,
		},
	});
	return {
		subject: entry.receipt.subject,
		job_id: entry.receipt.job_id,
		candidate_index: entry.receipt.asset.candidate_index,
		provider: entry.receipt.provider,
		generation: {
			receipt_path: approved('candidate-receipt.json'),
			receipt_sha256: entry.receiptFile.sha256,
			source_receipt_path: portablePath(entry.repoRoot, entry.receiptFile.path),
			prompt_path: approved('prompt.txt'),
			prompt_file_sha256: entry.promptFile.sha256,
			prompt_sha256: entry.receipt.provenance.prompt_sha256,
			input_record_path: approved('input-record.json'),
			input_record_file_sha256: entry.inputRecordFile.sha256,
			input_record_sha256: entry.receipt.subject.input_record_sha256,
			raw_artifact: {
				path: approved('provider-raw.png'),
				sha256: entry.providerRaw.sha256,
				source_path: portablePath(entry.repoRoot, entry.providerRaw.path),
			},
			reprocessing: entry.receipt.reprocessing ?? null,
			timing: entry.receipt.timing ?? null,
		},
		art: {
			portrait: childManifest('portrait'),
			marker: childManifest('marker'),
		},
		approval: {
			decision_path: approved('review-decision.json'),
			decision_sha256: entry.decisionFile.sha256,
			source_decision_path: portablePath(
				entry.repoRoot,
				entry.decisionFile.path,
			),
			pointer_path: portablePath(entry.repoRoot, entry.pointerFile.path),
			pointer_sha256: entry.pointerFile.sha256,
			review_id: entry.decision.review_id,
			decision: entry.decision.decision,
			promotion_eligible: entry.decision.promotion_eligible,
			reviewer: entry.decision.reviewer,
			reviewed_at: entry.decision.reviewed_at,
			notes: entry.decision.notes,
			checklist: entry.decision.checklist,
			hair_topology_sha256: entry.hairTopology.sha256,
			marker_identity_sha256: entry.markerIdentity.sha256,
		},
	};
}

async function stageRelease(
	entries,
	outputRoot,
	mode,
	wholeCastReview,
	currentInputsFile,
) {
	const parent = dirname(outputRoot);
	await mkdir(parent, { recursive: true });
	if (await exists(outputRoot)) {
		throw new PromotionError(
			`Approved release already exists at ${outputRoot}`,
		);
	}
	const staging = join(
		parent,
		`.${basename(outputRoot)}.tmp-${process.pid}-${randomUUID()}`,
	);
	try {
		await mkdir(staging, { recursive: false });
		const first = entries[0];
		await Promise.all([
			copyExact(first.configFile, join(staging, 'generation-config.json')),
			copyExact(currentInputsFile, join(staging, 'npc-art-inputs.json')),
			copyExact(
				wholeCastReview.recordFile,
				join(staging, 'whole-cast-review.json'),
			),
		]);
		const references = [];
		for (const reference of first.references.toSorted((left, right) =>
			left.id.localeCompare(right.id),
		)) {
			const destination = join(
				staging,
				'references',
				`${reference.sha256}.png`,
			);
			await copyExact(reference.file, destination);
			references.push({
				id: reference.id,
				purpose: reference.purpose,
				path: portablePath(staging, destination),
				sha256: reference.sha256,
				source_path: portablePath(first.repoRoot, reference.file.path),
			});
		}

		const sortedEntries = entries.toSorted((left, right) => {
			if (left.key === 'fallback') return 1;
			if (right.key === 'fallback') return -1;
			return Number(left.key) - Number(right.key);
		});
		for (const entry of sortedEntries) {
			const personRoot = join(staging, 'people', entry.key);
			await Promise.all([
				copyExact(
					entry.art.portrait.candidate,
					join(personRoot, 'portrait.png'),
				),
				copyExact(entry.art.marker.candidate, join(personRoot, 'marker.png')),
				copyExact(entry.providerRaw, join(personRoot, 'provider-raw.png')),
				copyExact(entry.art.portrait.raw, join(personRoot, 'portrait-raw.png')),
				copyExact(entry.art.marker.raw, join(personRoot, 'marker-raw.png')),
				copyExact(entry.promptFile, join(personRoot, 'prompt.txt')),
				copyExact(entry.inputRecordFile, join(personRoot, 'input-record.json')),
				copyExact(
					entry.receiptFile,
					join(personRoot, 'candidate-receipt.json'),
				),
				copyExact(entry.decisionFile, join(personRoot, 'review-decision.json')),
			]);
		}

		const manifestBase = {
			schema_version: 1,
			manifest_type: 'notebook-person-art-approved-release',
			release_version: basename(outputRoot),
			mode,
			entry_count: sortedEntries.length,
			approval: {
				whole_cast_visual_review: {
					path: 'whole-cast-review.json',
					sha256: wholeCastReview.recordFile.sha256,
					review_id: wholeCastReview.record.review_id,
				},
			},
			provenance: {
				generation_config: {
					path: 'generation-config.json',
					sha256: first.configFile.sha256,
					source_path: portablePath(first.repoRoot, first.configFile.path),
				},
				npc_art_inputs: {
					path: 'npc-art-inputs.json',
					sha256: currentInputsFile.sha256,
					source_path: portablePath(first.repoRoot, currentInputsFile.path),
				},
				references,
			},
			entries: sortedEntries.map((entry) => entryManifest(entry, staging)),
		};
		const manifest = {
			...manifestBase,
			release_id: sha256(canonicalJson(manifestBase)),
		};
		await writeFile(
			join(staging, 'release-manifest.json'),
			prettyJson(manifest),
			{
				flag: 'wx',
			},
		);
		if (await exists(outputRoot)) {
			throw new PromotionError(
				`Approved release appeared concurrently at ${outputRoot}`,
			);
		}
		await rename(staging, outputRoot);
		return {
			manifest,
			manifestPath: join(outputRoot, 'release-manifest.json'),
		};
	} catch (error) {
		await rm(staging, { recursive: true, force: true });
		throw error;
	}
}

async function collectPacketInputs(repoRoot, packetInputs) {
	const receiptPaths = [];
	const decisionPaths = [];
	for (const packetInput of packetInputs) {
		const packetPath = resolvePath(repoRoot, packetInput);
		const packet = (await loadJson(packetPath, 'review packet manifest')).value;
		if (!Array.isArray(packet.templates) || packet.templates.length === 0) {
			throw new PromotionError(
				`Review packet manifest has no templates: ${packetPath}`,
			);
		}
		for (const templateInput of packet.templates) {
			const templatePath = isAbsolute(templateInput)
				? resolve(templateInput)
				: resolve(dirname(packetPath), templateInput);
			const template = (await loadJson(templatePath, 'review template')).value;
			const receiptPath = resolvePath(
				repoRoot,
				template.candidate_receipt_path,
			);
			const pointer = (
				await loadJson(
					join(dirname(receiptPath), 'review.json'),
					'review pointer',
				)
			).value;
			receiptPaths.push(receiptPath);
			decisionPaths.push(resolvePath(repoRoot, pointer.record_path));
		}
	}
	return { receiptPaths, decisionPaths };
}

export async function promoteNotebookPersonArt(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	const packetPaths = options.packetPaths ?? [];
	let receiptPaths = options.receiptPaths ?? [];
	let decisionPaths = options.decisionPaths ?? [];
	const mode = options.mode ?? 'production';
	if (mode !== 'production' && mode !== 'fixture') {
		throw new PromotionError('Promotion mode must be production or fixture');
	}
	if (
		packetPaths.length > 0 &&
		(receiptPaths.length > 0 || decisionPaths.length > 0)
	) {
		throw new PromotionError(
			'Use review packets or explicit receipt/decision pairs, not both',
		);
	}
	if (packetPaths.length > 0) {
		({ receiptPaths, decisionPaths } = await collectPacketInputs(
			repoRoot,
			packetPaths,
		));
	}
	if (
		receiptPaths.length === 0 ||
		receiptPaths.length !== decisionPaths.length
	) {
		throw new PromotionError(
			'Promotion requires the same nonzero number of --receipt and --decision paths',
		);
	}
	const entries = [];
	const artDirectionPath = resolvePath(
		repoRoot,
		options.artDirectionPath ?? canonicalArtDirectionPath,
	);
	if (
		mode === 'production' &&
		artDirectionPath !== resolve(repoRoot, canonicalArtDirectionPath)
	) {
		throw new PromotionError(
			'Production promotion requires the canonical schema-v4 NPC art direction sidecar',
		);
	}
	const artDirectionFile = await loadJson(
		artDirectionPath,
		'current schema-v4 NPC art direction sidecar',
	);
	for (let index = 0; index < receiptPaths.length; index += 1) {
		entries.push(
			await validatePair(
				repoRoot,
				receiptPaths[index],
				decisionPaths[index],
				artDirectionFile.value,
			),
		);
	}
	const currentInputsPath = resolvePath(
		repoRoot,
		options.inputsPath ?? canonicalInputsPath,
	);
	const currentInputsFile = await loadJson(
		currentInputsPath,
		'current NPC art inputs',
	);
	validateRelease(entries, mode, currentInputsFile);
	if (mode === 'production') {
		await validateCanonicalProductionConfig(entries, repoRoot);
		await validateCanonicalProductionInputs(repoRoot, currentInputsFile);
	}
	const wholeCastReview = await validateWholeCastReview(
		repoRoot,
		options.castReviewPath,
		entries,
		mode,
	);
	const outputRoot = resolvePath(repoRoot, options.outputDir ?? defaultOutput);
	return stageRelease(
		entries,
		outputRoot,
		mode,
		wholeCastReview,
		currentInputsFile,
	);
}

export function helpText() {
	return `Promote hash-bound notebook person-art approvals into an immutable release.

Usage:
  node scripts/promote-notebook-person-art.mjs \\
    --packet <review-packet/manifest.json> [--packet ...] \\
    --cast-review <whole-cast-review.json> \\
	    [--inputs <fixture-npc-art-inputs.json>] [--art-direction <npc-art-direction-v1.json>] [--output <directory>] [--mode production|fixture]

  node scripts/promote-notebook-person-art.mjs \\
    --receipt <candidate-receipt.json> --decision <review-decision.json> \\
    --cast-review <whole-cast-review.json> \\
	    [--receipt ... --decision ...] [--inputs <fixture-npc-art-inputs.json>] [--art-direction <npc-art-direction-v1.json>] [--output <directory>] [--mode production|fixture]

Options:
  --packet <path>    Review packet manifest. Repeat for independently reviewed batches.
                     Decisions resolve from each receipt's immutable review pointer.
  --receipt <path>   Candidate pair receipt. Repeat once per subject.
  --decision <path> Immutable approved review record paired by argument order.
	  --cast-review <path> One immutable decision binding the complete candidate set.
	  --inputs <path>    Fixture-only alternate NPC art inputs. Production always regenerates and verifies the canonical repository npc-art-inputs-v1.json.
	  --art-direction <path> Validated schema-v4 fixture sidecar; production always uses the canonical repo sidecar.
  --output <path>    Release directory (default: art/notebook-person-art/approved/v1).
  --mode <mode>      production (default) requires 23 numeric NPC ids plus fallback.
                     fixture permits a partial synthetic set for unpaid tests.
  --fixture          Alias for --mode fixture.
  --repo-root <path> Base for repository-relative provenance paths.
  --help             Show this help without reading candidate storage.

The command refuses an existing output directory and never modifies candidate storage.`;
}

function cliOptions() {
	const parsed = parseArgs({
		options: {
			packet: { type: 'string', multiple: true },
			receipt: { type: 'string', multiple: true },
			decision: { type: 'string', multiple: true },
			'cast-review': { type: 'string' },
			inputs: { type: 'string' },
			'art-direction': { type: 'string' },
			output: { type: 'string' },
			mode: { type: 'string' },
			fixture: { type: 'boolean' },
			'repo-root': { type: 'string' },
			help: { type: 'boolean', short: 'h' },
		},
		allowPositionals: false,
		strict: true,
	});
	if (parsed.values.fixture && parsed.values.mode) {
		throw new PromotionError('Use either --fixture or --mode, not both');
	}
	return {
		help: parsed.values.help,
		packetPaths: parsed.values.packet,
		receiptPaths: parsed.values.receipt,
		decisionPaths: parsed.values.decision,
		castReviewPath: parsed.values['cast-review'],
		inputsPath: parsed.values.inputs,
		artDirectionPath: parsed.values['art-direction'],
		outputDir: parsed.values.output,
		mode: parsed.values.fixture ? 'fixture' : parsed.values.mode,
		repoRoot: parsed.values['repo-root'],
	};
}

async function main() {
	const options = cliOptions();
	if (options.help) {
		console.log(helpText());
		return;
	}
	const result = await promoteNotebookPersonArt(options);
	console.log(
		prettyJson({
			release_id: result.manifest.release_id,
			entries: result.manifest.entry_count,
			manifest: portablePath(
				resolve(options.repoRoot ?? defaultRepoRoot),
				result.manifestPath,
			),
		}).trimEnd(),
	);
}

const isMain =
	process.argv[1] &&
	pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
	main().catch((error) => {
		console.error(error.message);
		process.exitCode = 1;
	});
}
