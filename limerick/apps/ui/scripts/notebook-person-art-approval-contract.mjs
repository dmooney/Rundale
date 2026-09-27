import { createHash } from 'node:crypto';

export const NAMED_CAST_SIZE = 23;
export const ART_DIRECTION_SCHEMA_VERSION = 4;

export const REQUIRED_PAIR_REVIEW_CHECKS = Object.freeze([
	'identity_specific',
	'concept_style_match',
	'period_appropriate',
	'no_prohibited_content',
	'key_removal_clean',
	'production_quality',
	'diegetic_player_sketch',
	'uncolored_pen_and_ink',
	'sparse_open_linework',
	'no_baked_background',
	'framing_complete',
	'selected_size_readable',
	'nearby_size_readable',
	'restrained_world_color',
	'full_body_complete',
	'silhouette_readable',
	'character_only_marker',
	'empty_hands',
	'no_contextual_props_or_scenery',
	'no_ground_plane',
	'scene_size_readable',
	'cross_asset_identity',
	'hair_front_matches',
	'hair_rear_matches',
	'hair_covering_matches',
	'hair_silhouette_matches',
	'correct_surface_split',
	'atomic_rerender_understood',
]);

export const REQUIRED_WHOLE_CAST_REVIEW_CHECKS = Object.freeze([
	'cast_distinctive',
	'cast_hair_topology_distinctive',
]);

export function sha256(value) {
	return createHash('sha256').update(value).digest('hex');
}

export function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) {
		return `[${value.map((child) => canonicalJson(child)).join(',')}]`;
	}
	return `{${Object.entries(value)
		.toSorted(([left], [right]) => left.localeCompare(right))
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

export function pairCandidateDigest(receipt) {
	return sha256(
		JSON.stringify({
			portrait: receipt.artifact?.children?.portrait?.candidate_sha256,
			marker: receipt.artifact?.children?.marker?.candidate_sha256,
		}),
	);
}

export function subjectKey(subject) {
	if (subject?.kind === 'fallback' && subject.npc_id === null) {
		return 'fallback';
	}
	if (
		subject?.kind === 'npc' &&
		Number.isInteger(subject.npc_id) &&
		subject.npc_id > 0
	) {
		return `npc:${subject.npc_id}`;
	}
	throw new Error(
		'Review subject must be a positive numeric NPC id or fallback',
	);
}

function assertExactObjectKeys(value, expectedKeys, label) {
	if (!value || typeof value !== 'object' || Array.isArray(value)) {
		throw new Error(`${label} must be an object`);
	}
	const actual = Object.keys(value).toSorted();
	const expected = [...expectedKeys].toSorted();
	if (canonicalJson(actual) !== canonicalJson(expected)) {
		throw new Error(`${label} keys do not match the expected schema`);
	}
}

function assertNonEmptyString(value, label) {
	if (typeof value !== 'string' || !value.trim()) {
		throw new Error(`${label} must be a non-empty string`);
	}
}

function validateHairTopologyVector(vector) {
	assertExactObjectKeys(
		vector,
		[
			'color_and_texture',
			'front',
			'rear',
			'covering',
			'silhouette',
			'loose_details',
		],
		'hair_topology',
	);
	assertNonEmptyString(
		vector.color_and_texture,
		'hair_topology.color_and_texture',
	);
	assertNonEmptyString(vector.loose_details, 'hair_topology.loose_details');
	for (const dimension of ['front', 'rear', 'covering', 'silhouette']) {
		const value = vector[dimension];
		assertExactObjectKeys(
			value,
			['family', 'description'],
			`hair_topology.${dimension}`,
		);
		assertNonEmptyString(value.family, `hair_topology.${dimension}.family`);
		assertNonEmptyString(
			value.description,
			`hair_topology.${dimension}.description`,
		);
	}
}

function validateMarkerIdentityRecord(record) {
	assertExactObjectKeys(
		record,
		[
			'composition',
			'silhouette',
			'stance',
			'empty_hand_pose',
			'readability_cues',
			'tiny_readability_notes',
		],
		'marker_identity',
	);
	for (const field of [
		'composition',
		'silhouette',
		'stance',
		'empty_hand_pose',
	]) {
		assertNonEmptyString(record[field], `marker_identity.${field}`);
	}
	if (record.composition !== 'character-only') {
		throw new Error('marker_identity.composition must be character-only');
	}
	if (
		!Array.isArray(record.readability_cues) ||
		record.readability_cues.length === 0
	) {
		throw new Error(
			'marker_identity.readability_cues must be a non-empty array',
		);
	}
	for (const [index, cue] of record.readability_cues.entries()) {
		assertExactObjectKeys(
			cue,
			['kind', 'description'],
			`marker_identity.readability_cues[${index}]`,
		);
		assertNonEmptyString(
			cue.kind,
			`marker_identity.readability_cues[${index}].kind`,
		);
		assertNonEmptyString(
			cue.description,
			`marker_identity.readability_cues[${index}].description`,
		);
	}
	if (
		!Array.isArray(record.tiny_readability_notes) ||
		record.tiny_readability_notes.length === 0
	) {
		throw new Error(
			'marker_identity.tiny_readability_notes must be a non-empty array',
		);
	}
	for (const [index, note] of record.tiny_readability_notes.entries()) {
		assertNonEmptyString(
			note,
			`marker_identity.tiny_readability_notes[${index}]`,
		);
	}
}

function artDirectionRecord(sidecar, subject) {
	if (sidecar?.schema_version !== ART_DIRECTION_SCHEMA_VERSION) {
		throw new Error(
			`NPC art direction must use schema v${ART_DIRECTION_SCHEMA_VERSION}`,
		);
	}
	const key = subjectKey(subject);
	if (key === 'fallback') {
		if (!sidecar.fallback) {
			throw new Error('NPC art direction must contain a fallback record');
		}
		return { key, record: sidecar.fallback };
	}
	if (!Array.isArray(sidecar.npcs)) {
		throw new Error(
			`NPC art direction schema v${ART_DIRECTION_SCHEMA_VERSION} must contain an npcs array`,
		);
	}
	const matches = sidecar.npcs.filter((npc) => npc?.npc_id === subject.npc_id);
	if (matches.length !== 1) {
		throw new Error(
			`NPC art direction must contain exactly one record for ${key}`,
		);
	}
	return { key, record: matches[0] };
}

export function hairTopologyBinding(sidecar, subject) {
	const { key, record } = artDirectionRecord(sidecar, subject);
	const vector = record?.portrait_identity?.hair_topology;
	validateHairTopologyVector(vector);
	const digestInput = {
		schema_version: ART_DIRECTION_SCHEMA_VERSION,
		subject_key: key,
		vector,
	};
	return { ...digestInput, sha256: sha256(canonicalJson(digestInput)) };
}

export function markerIdentityBinding(sidecar, subject) {
	const { key, record } = artDirectionRecord(sidecar, subject);
	const markerIdentity = record.marker_identity;
	validateMarkerIdentityRecord(markerIdentity);
	const digestInput = {
		schema_version: ART_DIRECTION_SCHEMA_VERSION,
		subject_key: key,
		marker_identity: markerIdentity,
	};
	return { ...digestInput, sha256: sha256(canonicalJson(digestInput)) };
}

export function assertCanonicalHairTopologyBinding(binding, subject, label) {
	assertExactObjectKeys(
		binding,
		['schema_version', 'subject_key', 'vector', 'sha256'],
		label,
	);
	if (binding.schema_version !== ART_DIRECTION_SCHEMA_VERSION) {
		throw new Error(
			`${label}.schema_version must be ${ART_DIRECTION_SCHEMA_VERSION}`,
		);
	}
	if (binding.subject_key !== subjectKey(subject)) {
		throw new Error(`${label}.subject_key does not match the review subject`);
	}
	validateHairTopologyVector(binding.vector);
	const { sha256: digest, ...digestInput } = binding;
	if (digest !== sha256(canonicalJson(digestInput))) {
		throw new Error(`${label}.sha256 does not match its canonical vector`);
	}
	return binding;
}

export function assertHairTopologyBinding(binding, sidecar, subject, label) {
	assertCanonicalHairTopologyBinding(binding, subject, label);
	const expected = hairTopologyBinding(sidecar, subject);
	if (canonicalJson(binding) !== canonicalJson(expected)) {
		throw new Error(`${label} does not match current schema-v4 hair topology`);
	}
	return expected;
}

export function assertCanonicalMarkerIdentityBinding(binding, subject, label) {
	assertExactObjectKeys(
		binding,
		['schema_version', 'subject_key', 'marker_identity', 'sha256'],
		label,
	);
	if (binding.schema_version !== ART_DIRECTION_SCHEMA_VERSION) {
		throw new Error(
			`${label}.schema_version must be ${ART_DIRECTION_SCHEMA_VERSION}`,
		);
	}
	if (binding.subject_key !== subjectKey(subject)) {
		throw new Error(`${label}.subject_key does not match the review subject`);
	}
	validateMarkerIdentityRecord(binding.marker_identity);
	const { sha256: digest, ...digestInput } = binding;
	if (digest !== sha256(canonicalJson(digestInput))) {
		throw new Error(`${label}.sha256 does not match its canonical record`);
	}
	return binding;
}

export function assertMarkerIdentityBinding(binding, sidecar, subject, label) {
	assertCanonicalMarkerIdentityBinding(binding, subject, label);
	const expected = markerIdentityBinding(sidecar, subject);
	if (canonicalJson(binding) !== canonicalJson(expected)) {
		throw new Error(
			`${label} does not match current schema-v4 marker identity`,
		);
	}
	return expected;
}

export function reviewRecordBase(record) {
	const { review_id: _reviewId, ...base } = record;
	return base;
}

export function computeReviewId(record) {
	return sha256(JSON.stringify(reviewRecordBase(record))).slice(0, 24);
}

export function assertExactChecklist(checklist, expectedKeys, label) {
	if (!checklist || typeof checklist !== 'object' || Array.isArray(checklist)) {
		throw new Error(`${label} must be an object`);
	}
	const expected = [...expectedKeys].toSorted();
	const actual = Object.keys(checklist).toSorted();
	if (canonicalJson(actual) !== canonicalJson(expected)) {
		throw new Error(`${label} keys do not match the approval contract`);
	}
	for (const key of expected) {
		if (checklist[key] !== true) {
			throw new Error(`${label}.${key} must be true`);
		}
	}
}

export function castMemberFromReceipt(
	receipt,
	receiptPath,
	receiptSha256,
	hairTopology,
	markerIdentity,
) {
	return {
		subject_key: subjectKey(receipt.subject),
		subject: receipt.subject,
		candidate_receipt_path: receiptPath,
		candidate_receipt_sha256: receiptSha256,
		candidate_sha256: pairCandidateDigest(receipt),
		raw_sha256: receipt.artifact?.raw_sha256,
		portrait_sha256: receipt.artifact?.children?.portrait?.candidate_sha256,
		marker_sha256: receipt.artifact?.children?.marker?.candidate_sha256,
		hair_topology: hairTopology,
		marker_identity: markerIdentity,
	};
}

export function sortCastMembers(members) {
	return [...members].toSorted((left, right) => {
		if (left.subject_key === 'fallback') return 1;
		if (right.subject_key === 'fallback') return -1;
		return left.subject.npc_id - right.subject.npc_id;
	});
}

export function castBinding(members) {
	return {
		named_count: members.filter((member) => member.subject?.kind === 'npc')
			.length,
		total_count: members.length,
		members: sortCastMembers(members),
	};
}
