import { randomUUID } from 'node:crypto';
import { mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
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
import {
	assertHairTopologyBinding,
	assertMarkerIdentityBinding,
	castBinding,
	castMemberFromReceipt,
	canonicalJson,
	computeReviewId,
	hairTopologyBinding,
	markerIdentityBinding,
	NAMED_CAST_SIZE,
	pairCandidateDigest,
	REQUIRED_PAIR_REVIEW_CHECKS,
	REQUIRED_WHOLE_CAST_REVIEW_CHECKS,
	sha256,
} from './notebook-person-art-approval-contract.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(here, '..');
const defaultRepoRoot = resolve(uiRoot, '../../..');
const defaultCandidateRoot = join(
	uiRoot,
	'art',
	'notebook-person-art',
	'candidates',
);
const defaultArtDirectionPath = join(
	uiRoot,
	'art',
	'notebook-person-art',
	'npc-art-direction-v1.json',
);
const MAX_PACKET_CANDIDATES = 8;
const MAX_PACKET_IMAGE_BYTES = 96 * 1024 * 1024;
const MAX_CAST_PACKET_COUNT = 3;
const MAX_CAST_REVIEW_IMAGE_BYTES = 32 * 1024 * 1024;

class ReviewError extends Error {}

function prettyJson(value) {
	return `${JSON.stringify(value, null, '\t')}\n`;
}

function resolvePath(repoRoot, configuredPath) {
	return isAbsolute(configuredPath)
		? configuredPath
		: resolve(repoRoot, configuredPath);
}

function portablePath(repoRoot, path) {
	const local = relative(repoRoot, path);
	return local && !local.startsWith('..') ? local.replaceAll('\\', '/') : path;
}

async function fileExists(path) {
	try {
		await stat(path);
		return true;
	} catch (error) {
		if (error.code === 'ENOENT') return false;
		throw error;
	}
}

async function atomicWrite(path, value) {
	await mkdir(dirname(path), { recursive: true });
	const temporary = `${path}.tmp-${process.pid}-${randomUUID()}`;
	try {
		await writeFile(temporary, value, { flag: 'wx' });
		await rename(temporary, path);
	} finally {
		await rm(temporary, { force: true });
	}
}

async function loadJson(path, label) {
	let bytes;
	try {
		bytes = await readFile(path);
	} catch (error) {
		throw new ReviewError(`Could not read ${label} ${path}: ${error.message}`);
	}
	let value;
	try {
		value = JSON.parse(bytes.toString('utf8'));
	} catch (error) {
		throw new ReviewError(`Could not parse ${label} ${path}: ${error.message}`);
	}
	return { value, bytes, sha256: sha256(bytes) };
}

async function loadArtDirection(repoRoot, pathInput, fixture) {
	const path = resolvePath(repoRoot, pathInput ?? defaultArtDirectionPath);
	if (path !== resolve(defaultArtDirectionPath) && !fixture) {
		throw new ReviewError(
			'Non-canonical NPC art direction sidecars require explicit fixture mode',
		);
	}
	return (await loadJson(path, 'canonical NPC art direction sidecar')).value;
}

function identityBindings(sidecar, subject, existing = null) {
	try {
		return {
			hairTopology: existing
				? assertHairTopologyBinding(
						existing.hairTopology,
						sidecar,
						subject,
						`${existing.label} hair topology`,
					)
				: hairTopologyBinding(sidecar, subject),
			markerIdentity: existing
				? assertMarkerIdentityBinding(
						existing.markerIdentity,
						sidecar,
						subject,
						`${existing.label} marker identity`,
					)
				: markerIdentityBinding(sidecar, subject),
		};
	} catch (error) {
		throw new ReviewError(error.message);
	}
}

function assertPng(bytes, label) {
	if (bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a') {
		throw new ReviewError(`${label} is not a PNG`);
	}
}

const sharedChecklist = {
	identity_specific:
		'The asset is specific to this NPC record, not a generic variant.',
	concept_style_match:
		'The asset belongs to the illustrated notebook concept art direction.',
	period_appropriate:
		'Clothing, hair, and pose are plausible for rural Roscommon in 1820.',
	no_prohibited_content:
		'There is no text, watermark, frame, modern object, fantasy cue, or extra person.',
	key_removal_clean:
		'The transparent candidate has clean edges without a visible magenta halo.',
	production_quality:
		'The asset is finished production art rather than a placeholder or experiment.',
};

const portraitChecklist = {
	...sharedChecklist,
	diegetic_player_sketch:
		'The portrait plausibly looks like a quick observational sketch made by the player character in a working notebook, not professional character illustration.',
	uncolored_pen_and_ink:
		'The portrait is uncolored pen-and-ink with only minimal monochrome value shading.',
	sparse_open_linework:
		'Most of the face, hair, clothing, and canvas remain open and unfilled, with economical contours and no dense hatching or tonal modeling.',
	no_baked_background:
		'The portrait contains no parchment, paper texture, card, frame, or background wash.',
	framing_complete:
		'The top of the hair or head covering and the intended shoulders are fully readable.',
	selected_size_readable:
		'The portrait reads clearly at the selected-person size.',
	nearby_size_readable:
		'The portrait remains distinct at the Nearby rail size.',
};

const markerChecklist = {
	...sharedChecklist,
	restrained_world_color:
		'Watercolor is restrained and uses the concept palette without saturated accents.',
	full_body_complete:
		'The full body and both feet are visible without cropping.',
	silhouette_readable:
		'The NPC is recognizable from face, hair or headwear, clothing, body shape, and stance.',
	character_only_marker:
		'The marker is one transparent character-only cutout, not a miniature narrative vignette.',
	empty_hands: 'Both hands are empty and no object is held or carried.',
	no_contextual_props_or_scenery:
		'There is no extra person, furniture, counter, architecture, vegetation, scenery fragment, ground plane, or shadow.',
	no_ground_plane:
		'There is no baked floor, card, frame, label, or broad ground shadow.',
	scene_size_readable:
		'The marker remains legible at its in-scene runtime size.',
};

const pairChecklist = {
	...portraitChecklist,
	...markerChecklist,
	cross_asset_identity:
		'The portrait and marker unmistakably depict the same person: matching apparent age, facial proportions, eyes, nose, jaw, hairline, hairstyle, and characteristic expression.',
	hair_front_matches:
		'The visible front arrangement matches the reviewed source record in both assets; a generic centre part, fringe, roll, curl, or covered hairline is not substituted.',
	hair_rear_matches:
		'The visible rear anchor, height, and geometry match the reviewed source record in both assets; a generic low bun or rounded coil is not substituted.',
	hair_covering_matches:
		'The exact cap, bonnet, kerchief, shawl, or uncovered state matches the reviewed source record and remains consistent across both assets.',
	hair_silhouette_matches:
		'The complete hair and headwear silhouette matches the reviewed source record and stays recognizable at portrait and marker review sizes.',
	correct_surface_split:
		'The left portrait remains sparse uncolored notebook ink while the right marker alone uses restrained painted-world watercolor.',
	atomic_rerender_understood:
		'If either child fails review, this entire portrait-marker pair will be rejected and rerun together.',
};

const wholeCastChecklist = {
	cast_distinctive:
		'Every named face plus the fallback is visibly distinct across the complete bound cast, including apparent age, facial geometry, hair, and silhouette.',
	cast_hair_topology_distinctive:
		'Across the complete bound cast, hair front, rear anchor, covering, and overall silhouette form visibly distinct topologies rather than wording or color variants of one repeated construction.',
};

if (
	canonicalJson(Object.keys(pairChecklist).toSorted()) !==
	canonicalJson([...REQUIRED_PAIR_REVIEW_CHECKS].toSorted())
) {
	throw new Error('Pair review checklist drifted from the approval contract');
}
if (
	canonicalJson(Object.keys(wholeCastChecklist).toSorted()) !==
	canonicalJson([...REQUIRED_WHOLE_CAST_REVIEW_CHECKS].toSorted())
) {
	throw new Error('Whole-cast checklist drifted from the approval contract');
}

function checklistDefinition(assetKind) {
	if (assetKind === 'portrait') return portraitChecklist;
	if (assetKind === 'marker') return markerChecklist;
	if (assetKind === 'pair') return pairChecklist;
	throw new ReviewError(`Unsupported candidate asset kind ${assetKind}`);
}

function candidateDigest(receipt) {
	if (receipt.asset?.kind === 'pair') return pairCandidateDigest(receipt);
	return receipt.artifact?.candidate_sha256;
}

async function loadCandidate(repoRoot, receiptPathInput) {
	const receiptPath = resolvePath(repoRoot, receiptPathInput);
	const receiptFile = await loadJson(receiptPath, 'candidate receipt');
	const receipt = receiptFile.value;
	if (
		receipt.schema_version !== 1 ||
		(receipt.receipt_type !== 'notebook-person-art-candidate' &&
			receipt.receipt_type !== 'notebook-person-art-pair-candidate') ||
		receipt.status !== 'candidate'
	) {
		throw new ReviewError(`${receiptPath} is not a v1 candidate receipt`);
	}
	if (
		receipt.review?.status !== 'pending' ||
		receipt.promotion?.eligible !== false
	) {
		throw new ReviewError(
			'Candidate receipt must remain pending and ineligible',
		);
	}
	const rawPath = resolvePath(repoRoot, receipt.artifact?.raw_path ?? '');
	if (!receipt.artifact?.raw_path) {
		throw new ReviewError('Candidate receipt is missing its raw artifact path');
	}
	const raw = await readFile(rawPath);
	assertPng(raw, 'Raw artifact');
	if (sha256(raw) !== receipt.artifact.raw_sha256) {
		throw new ReviewError('Raw artifact hash does not match its receipt');
	}

	const digest = candidateDigest(receipt);
	if (!digest)
		throw new ReviewError('Candidate receipt has no candidate digest');
	if (receipt.asset?.kind === 'pair') {
		const children = {};
		for (const kind of ['portrait', 'marker']) {
			const child = receipt.artifact.children?.[kind];
			if (!child?.raw_path || !child?.candidate_path) {
				throw new ReviewError(`Pair receipt is missing ${kind} artifact paths`);
			}
			const childRawPath = resolvePath(repoRoot, child.raw_path);
			const childCandidatePath = resolvePath(repoRoot, child.candidate_path);
			const [childRaw, childCandidate] = await Promise.all([
				readFile(childRawPath),
				readFile(childCandidatePath),
			]);
			assertPng(childRaw, `${kind} raw artifact`);
			assertPng(childCandidate, `${kind} candidate artifact`);
			if (sha256(childRaw) !== child.raw_sha256) {
				throw new ReviewError(`${kind} raw hash does not match its receipt`);
			}
			if (sha256(childCandidate) !== child.candidate_sha256) {
				throw new ReviewError(
					`${kind} candidate hash does not match its receipt`,
				);
			}
			children[kind] = {
				rawPath: childRawPath,
				raw: childRaw,
				candidatePath: childCandidatePath,
				candidate: childCandidate,
			};
		}
		return {
			receipt,
			receiptPath,
			receiptBytes: receiptFile.bytes,
			receiptSha256: receiptFile.sha256,
			rawPath,
			raw,
			candidatePath: null,
			candidate: null,
			candidateSha256: digest,
			children,
			reviewStorageRoot: dirname(receiptPath),
			imageBytes:
				raw.length +
				Object.values(children).reduce(
					(total, child) => total + child.raw.length + child.candidate.length,
					0,
				),
		};
	}

	const candidatePath = resolvePath(
		repoRoot,
		receipt.artifact?.candidate_path ?? '',
	);
	if (!receipt.artifact?.candidate_path) {
		throw new ReviewError(
			'Candidate receipt is missing candidate artifact path',
		);
	}
	const candidate = await readFile(candidatePath);
	assertPng(candidate, 'Candidate artifact');
	if (sha256(candidate) !== receipt.artifact.candidate_sha256) {
		throw new ReviewError('Candidate artifact hash does not match its receipt');
	}
	return {
		receipt,
		receiptPath,
		receiptBytes: receiptFile.bytes,
		receiptSha256: receiptFile.sha256,
		rawPath,
		raw,
		candidatePath,
		candidate,
		candidateSha256: digest,
		children: null,
		reviewStorageRoot: dirname(candidatePath),
		imageBytes: raw.length + candidate.length,
	};
}

function reviewTemplate(repoRoot, loaded, bindings) {
	const definition = checklistDefinition(loaded.receipt.asset.kind);
	return {
		schema_version: 1,
		template_type: 'notebook-person-art-human-review',
		candidate_receipt_path: portablePath(repoRoot, loaded.receiptPath),
		candidate_receipt_sha256: loaded.receiptSha256,
		candidate_sha256: loaded.candidateSha256,
		raw_sha256: loaded.receipt.artifact.raw_sha256,
		subject: loaded.receipt.subject,
		asset: loaded.receipt.asset,
		...(bindings
			? {
					hair_topology: bindings.hairTopology,
					marker_identity: bindings.markerIdentity,
				}
			: {}),
		decision: null,
		reviewer: null,
		notes: '',
		checklist: Object.fromEntries(
			Object.keys(definition).map((key) => [key, null]),
		),
	};
}

function escapeHtml(value) {
	return String(value)
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&#39;');
}

function dataUri(bytes) {
	return `data:image/png;base64,${bytes.toString('base64')}`;
}

function markerIdentityHtml(binding) {
	const record = binding.marker_identity;
	const cues = record.readability_cues
		.map(
			(cue) =>
				`<li><strong>${escapeHtml(cue.kind)}</strong>: ${escapeHtml(cue.description)}</li>`,
		)
		.join('\n');
	const notes = record.tiny_readability_notes
		.map((note) => `<li>${escapeHtml(note)}</li>`)
		.join('\n');
	return `<div class="identity-contract">
	<h3>Expected marker identity</h3>
	<dl>
		<dt>Canonical SHA-256</dt><dd><code>${escapeHtml(binding.sha256)}</code></dd>
		<dt>Composition</dt><dd>${escapeHtml(record.composition)}</dd>
		<dt>Silhouette</dt><dd>${escapeHtml(record.silhouette)}</dd>
		<dt>Stance</dt><dd>${escapeHtml(record.stance)}</dd>
		<dt>Empty-hand pose</dt><dd>${escapeHtml(record.empty_hand_pose)}</dd>
	</dl>
	<h4>Readability cues</h4><ul>${cues}</ul>
	<h4>Tiny-readability notes</h4><ul>${notes}</ul>
</div>`;
}

function packetHtml(entries) {
	const sections = entries
		.map(({ loaded, template, templateName }) => {
			const receipt = loaded.receipt;
			const definition = checklistDefinition(receipt.asset.kind);
			const checklist = Object.entries(definition)
				.map(
					([key, label]) =>
						`<li><code>${escapeHtml(key)}</code> ${escapeHtml(label)}</li>`,
				)
				.join('\n');
			if (receipt.asset.kind === 'pair') {
				const portrait = loaded.children.portrait.candidate;
				const marker = loaded.children.marker.candidate;
				return `<section>
	<h2>${escapeHtml(receipt.subject.name)}: identity-locked portrait + marker</h2>
	<p>Pair candidate ${receipt.asset.candidate_index}; one provider request; review template <code>${escapeHtml(templateName)}</code></p>
	<div class="proof-band pair">
		<figure class="source"><img src="${dataUri(portrait)}" alt="Transparent notebook portrait on UI parchment"><figcaption>Left cell: notebook portrait</figcaption></figure>
		<figure class="source"><img src="${dataUri(marker)}" alt="Transparent painted-world marker"><figcaption>Right cell: painted-world marker</figcaption></figure>
		<figure class="sheet keyed"><img src="${dataUri(loaded.raw)}" alt="Preserved paired keyed provider response"><figcaption>Preserved 2-cell provider response</figcaption></figure>
	</div>
	<div class="runtime-band">
		<figure><img src="${dataUri(portrait)}" width="99" height="112" alt="Selected portrait preview"><figcaption>Portrait 99x112</figcaption></figure>
		<figure><img src="${dataUri(portrait)}" width="51" height="57" alt="Nearby portrait preview"><figcaption>Portrait 51x57</figcaption></figure>
		<figure><img src="${dataUri(marker)}" width="60" height="85" alt="Scene marker preview"><figcaption>Marker 60x85</figcaption></figure>
	</div>
	<dl>
		<dt>Pair SHA-256</dt><dd><code>${escapeHtml(loaded.candidateSha256)}</code></dd>
		<dt>Portrait SHA-256</dt><dd><code>${escapeHtml(receipt.artifact.children.portrait.candidate_sha256)}</code></dd>
		<dt>Marker SHA-256</dt><dd><code>${escapeHtml(receipt.artifact.children.marker.candidate_sha256)}</code></dd>
		<dt>Model</dt><dd>${escapeHtml(receipt.provider.model)}</dd>
		<dt>Request ID</dt><dd><code>${escapeHtml(receipt.provider.request_id)}</code></dd>
	</dl>
	${markerIdentityHtml(template.marker_identity)}
	<h3>Required atomic checklist</h3>
	<ul>${checklist}</ul>
</section>`;
			}
			const selectedWidth = receipt.asset.kind === 'portrait' ? 99 : 120;
			const selectedHeight = receipt.asset.kind === 'portrait' ? 112 : 170;
			const tinyWidth = receipt.asset.kind === 'portrait' ? 51 : 60;
			const tinyHeight = receipt.asset.kind === 'portrait' ? 57 : 85;
			return `<section>
	<h2>${escapeHtml(receipt.subject.name)}: ${escapeHtml(receipt.asset.kind)}</h2>
	<p>Candidate ${receipt.asset.candidate_index}; review template <code>${escapeHtml(templateName)}</code></p>
	<div class="proof-band">
		<figure class="source"><img src="${dataUri(loaded.candidate)}" alt="Transparent candidate on UI parchment"><figcaption>Transparent candidate on parchment</figcaption></figure>
		<figure class="source keyed"><img src="${dataUri(loaded.raw)}" alt="Raw keyed provider response"><figcaption>Preserved raw provider response</figcaption></figure>
		<figure><img src="${dataUri(loaded.candidate)}" width="${selectedWidth}" height="${selectedHeight}" alt="Selected runtime preview"><figcaption>${selectedWidth}x${selectedHeight}</figcaption></figure>
		<figure><img src="${dataUri(loaded.candidate)}" width="${tinyWidth}" height="${tinyHeight}" alt="Tiny runtime preview"><figcaption>${tinyWidth}x${tinyHeight}</figcaption></figure>
	</div>
	<dl>
		<dt>Candidate SHA-256</dt><dd><code>${escapeHtml(receipt.artifact.candidate_sha256)}</code></dd>
		<dt>Model</dt><dd>${escapeHtml(receipt.provider.model)}</dd>
		<dt>Request ID</dt><dd><code>${escapeHtml(receipt.provider.request_id)}</code></dd>
	</dl>
	<h3>Required checklist</h3>
	<ul>${checklist}</ul>
</section>`;
		})
		.join('\n');
	return `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rundale Notebook Person Art Review</title>
<style>
:root { color-scheme: light; font-family: Georgia, serif; color: #36362e; background: #d7c6a7; }
body { margin: 0; }
header, section { padding: 24px max(24px, calc((100vw - 1180px) / 2)); border-bottom: 1px solid #807661; }
h1, h2, h3, p { max-width: 850px; }
.proof-band { display: grid; grid-template-columns: minmax(240px, 1fr) minmax(240px, 1fr) 160px 110px; gap: 18px; align-items: end; }
.proof-band.pair { grid-template-columns: minmax(240px, 1fr) minmax(240px, 1fr); }
.proof-band.pair .sheet { grid-column: 1 / -1; }
.proof-band.pair .sheet img { width: 100%; aspect-ratio: 2; }
.runtime-band { display: flex; gap: 18px; align-items: end; margin-top: 18px; }
.identity-contract { max-width: 900px; padding: 16px 0; }
figure { margin: 0; min-width: 0; }
figure img { display: block; max-width: 100%; object-fit: contain; background: #deccae; border: 1px solid #807661; }
figure.source img { width: 100%; aspect-ratio: 1; }
figure.keyed img { background: #ff00ff; }
figcaption { margin-top: 7px; font-size: 14px; }
dl { display: grid; grid-template-columns: 180px minmax(0, 1fr); gap: 6px 12px; }
dt { font-weight: 700; }
dd { margin: 0; overflow-wrap: anywhere; }
li { margin: 8px 0; max-width: 900px; }
code { font-family: ui-monospace, monospace; font-size: 0.9em; }
@media (max-width: 760px) {
	.proof-band { grid-template-columns: 1fr 1fr; }
	dl { grid-template-columns: 1fr; }
}
</style>
<header>
	<h1>Notebook Person Art Review</h1>
	<p>Generation does not imply approval. Record the decision in the hash-bound JSON template shown with each candidate.</p>
</header>
${sections}
</html>
`;
}

export async function prepareReviewPacket(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	const receiptInputs = options.receiptPaths ?? [];
	if (receiptInputs.length === 0) {
		throw new ReviewError('prepare requires at least one --receipt');
	}
	if (receiptInputs.length > MAX_PACKET_CANDIDATES) {
		throw new ReviewError(
			`Review packets are capped at ${MAX_PACKET_CANDIDATES} candidates`,
		);
	}
	const loaded = await Promise.all(
		receiptInputs.map((path) => loadCandidate(repoRoot, path)),
	);
	const hasPairs = loaded.some(
		(candidate) => candidate.receipt.asset.kind === 'pair',
	);
	const artDirection = hasPairs
		? await loadArtDirection(
				repoRoot,
				options.artDirectionPath,
				options.fixture === true,
			)
		: null;
	const imageBytes = loaded.reduce(
		(total, candidate) => total + candidate.imageBytes,
		0,
	);
	if (imageBytes > MAX_PACKET_IMAGE_BYTES) {
		throw new ReviewError('Review packet images exceed the 96 MiB safety cap');
	}
	const packetId =
		options.packetId ?? `review-${new Date().toISOString().slice(0, 10)}`;
	if (!/^[a-zA-Z0-9._-]+$/.test(packetId)) {
		throw new ReviewError('Packet ID contains unsupported characters');
	}
	const outputDir = resolvePath(
		repoRoot,
		options.outputDir ?? join(defaultCandidateRoot, 'review-packets', packetId),
	);
	await mkdir(outputDir, { recursive: true });
	const entries = [];
	for (const candidate of loaded) {
		const bindings =
			candidate.receipt.asset.kind === 'pair'
				? identityBindings(artDirection, candidate.receipt.subject)
				: null;
		const template = reviewTemplate(repoRoot, candidate, bindings);
		const templateName = `${candidate.receipt.subject.npc_id ?? 'fallback'}-${candidate.receipt.asset.kind}-${candidate.candidateSha256.slice(0, 12)}-review.json`;
		const templatePath = join(outputDir, templateName);
		await atomicWrite(templatePath, prettyJson(template));
		entries.push({ loaded: candidate, template, templateName, templatePath });
	}
	const htmlPath = join(outputDir, 'review.html');
	const manifestPath = join(outputDir, 'manifest.json');
	await Promise.all([
		atomicWrite(htmlPath, packetHtml(entries)),
		atomicWrite(
			manifestPath,
			prettyJson({
				schema_version: 1,
				packet_id: packetId,
				created_at: new Date().toISOString(),
				candidate_count: entries.length,
				html: basename(htmlPath),
				templates: entries.map((entry) => entry.templateName),
			}),
		),
	]);
	return {
		packetId,
		outputDir,
		htmlPath,
		manifestPath,
		templatePaths: entries.map((entry) => entry.templatePath),
	};
}

async function loadReviewPackets(repoRoot, packetInputs, artDirection) {
	if (packetInputs.length === 0) {
		throw new ReviewError('prepare-cast requires at least one --packet');
	}
	if (packetInputs.length > MAX_CAST_PACKET_COUNT) {
		throw new ReviewError(
			`Whole-cast review accepts at most ${MAX_CAST_PACKET_COUNT} visual packets`,
		);
	}
	const packets = [];
	const candidates = [];
	for (const packetInput of packetInputs) {
		const packetPath = resolvePath(repoRoot, packetInput);
		const packetFile = await loadJson(packetPath, 'review packet manifest');
		const packet = packetFile.value;
		if (
			packet.schema_version !== 1 ||
			!Array.isArray(packet.templates) ||
			packet.templates.length === 0 ||
			packet.templates.length > MAX_PACKET_CANDIDATES ||
			packet.candidate_count !== packet.templates.length
		) {
			throw new ReviewError(
				`Review packet must contain 1-${MAX_PACKET_CANDIDATES} bound templates`,
			);
		}
		packets.push({
			path: portablePath(repoRoot, packetPath),
			sha256: packetFile.sha256,
		});
		for (const templateInput of packet.templates) {
			const templatePath = isAbsolute(templateInput)
				? resolve(templateInput)
				: resolve(dirname(packetPath), templateInput);
			const template = (await loadJson(templatePath, 'review template')).value;
			if (
				template.schema_version !== 1 ||
				template.template_type !== 'notebook-person-art-human-review'
			) {
				throw new ReviewError(`${templatePath} is not a v1 review template`);
			}
			const loaded = await loadCandidate(
				repoRoot,
				template.candidate_receipt_path,
			);
			if (
				template.candidate_receipt_sha256 !== loaded.receiptSha256 ||
				template.candidate_sha256 !== loaded.candidateSha256
			) {
				throw new ReviewError(
					'Visual packet template no longer matches its candidate',
				);
			}
			if (loaded.receipt.asset.kind !== 'pair') {
				throw new ReviewError('Whole-cast review requires pair candidates');
			}
			const bindings = identityBindings(artDirection, loaded.receipt.subject, {
				hairTopology: template.hair_topology,
				markerIdentity: template.marker_identity,
				label: 'Pair review template',
			});
			loaded.hairTopology = bindings.hairTopology;
			loaded.markerIdentity = bindings.markerIdentity;
			candidates.push(loaded);
		}
	}
	return { packets, candidates };
}

function validateWholeCastCandidates(candidates, expectedNamedCount) {
	const members = candidates.map((candidate) =>
		castMemberFromReceipt(
			candidate.receipt,
			candidate.receiptPath,
			candidate.receiptSha256,
			candidate.hairTopology,
			candidate.markerIdentity,
		),
	);
	const binding = castBinding(members);
	const keys = binding.members.map((member) => member.subject_key);
	if (new Set(keys).size !== keys.length) {
		throw new ReviewError('Whole-cast review contains a duplicate subject');
	}
	const fallbackCount = binding.members.filter(
		(member) => member.subject_key === 'fallback',
	).length;
	if (
		binding.named_count !== expectedNamedCount ||
		binding.total_count !== expectedNamedCount + 1 ||
		fallbackCount !== 1
	) {
		throw new ReviewError(
			`Whole-cast review requires exactly ${expectedNamedCount} named candidates plus fallback`,
		);
	}
	return binding;
}

function wholeCastHtml(candidates, binding) {
	const byCandidate = new Map(
		candidates.map((candidate) => [candidate.candidateSha256, candidate]),
	);
	const figures = binding.members
		.map((member) => {
			const candidate = byCandidate.get(member.candidate_sha256);
			return `<figure>
	<img src="${dataUri(candidate.children.portrait.candidate)}" alt="${escapeHtml(member.subject.name)} portrait">
	<img src="${dataUri(candidate.children.marker.candidate)}" alt="${escapeHtml(member.subject.name)} marker">
	<figcaption>${escapeHtml(member.subject.name)} <code>${escapeHtml(member.candidate_sha256.slice(0, 12))}</code></figcaption>
	<details><summary>Expected marker identity</summary>${markerIdentityHtml(member.marker_identity)}</details>
</figure>`;
		})
		.join('\n');
	return `<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rundale Whole-Cast Person Art Review</title>
<style>
body { margin: 24px; color: #36362e; background: #d7c6a7; font-family: Georgia, serif; }
.cast { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 16px; }
figure { margin: 0; padding: 12px; border: 1px solid #807661; }
img { width: 46%; height: 136px; object-fit: contain; vertical-align: bottom; }
figcaption { margin-top: 8px; overflow-wrap: anywhere; }
details { margin-top: 8px; }
.identity-contract { font-size: 14px; overflow-wrap: anywhere; }
.identity-contract h3 { font-size: 16px; }
.identity-contract dl { display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: 4px 8px; }
.identity-contract dd { margin: 0; }
code { font-family: ui-monospace, monospace; }
@media (max-width: 760px) { .cast { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
<h1>Whole-Cast Visual Review</h1>
<p>One immutable decision binds all ${binding.total_count} candidates. Use the three source packets for full-size inspection and this complete set for cross-cast comparison.</p>
<div class="cast">${figures}</div>
`;
}

export async function prepareWholeCastReview(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	const expectedNamedCount = options.expectedNamedCount ?? NAMED_CAST_SIZE;
	const artDirection = await loadArtDirection(
		repoRoot,
		options.artDirectionPath,
		options.fixture === true,
	);
	const { packets, candidates } = await loadReviewPackets(
		repoRoot,
		options.packetPaths ?? [],
		artDirection,
	);
	const binding = validateWholeCastCandidates(candidates, expectedNamedCount);
	const imageBytes = candidates.reduce(
		(total, candidate) =>
			total +
			candidate.children.portrait.candidate.length +
			candidate.children.marker.candidate.length,
		0,
	);
	if (imageBytes > MAX_CAST_REVIEW_IMAGE_BYTES) {
		throw new ReviewError('Whole-cast preview images exceed the 32 MiB cap');
	}
	const outputDir = resolvePath(
		repoRoot,
		options.outputDir ?? join(defaultCandidateRoot, 'whole-cast-review'),
	);
	const templatePath = join(outputDir, 'whole-cast-review.json');
	const htmlPath = join(outputDir, 'whole-cast-review.html');
	const template = {
		schema_version: 1,
		template_type: 'notebook-person-art-whole-cast-human-review',
		source_packets: packets,
		cast: binding,
		decision: null,
		reviewer: null,
		notes: '',
		checklist: Object.fromEntries(
			REQUIRED_WHOLE_CAST_REVIEW_CHECKS.map((key) => [key, null]),
		),
	};
	await Promise.all([
		atomicWrite(templatePath, prettyJson(template)),
		atomicWrite(htmlPath, wholeCastHtml(candidates, binding)),
	]);
	return { template, templatePath, htmlPath };
}

export async function submitWholeCastReviewDecision(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	if (!options.templatePath) {
		throw new ReviewError('decide-cast requires --template');
	}
	const templatePath = resolvePath(repoRoot, options.templatePath);
	const template = (await loadJson(templatePath, 'whole-cast review template'))
		.value;
	if (
		template.schema_version !== 1 ||
		template.template_type !== 'notebook-person-art-whole-cast-human-review'
	) {
		throw new ReviewError(
			`${templatePath} is not a whole-cast review template`,
		);
	}
	const packetPaths = (template.source_packets ?? []).map((packet) => {
		if (!packet?.path || !packet?.sha256) {
			throw new ReviewError(
				'Whole-cast template has invalid packet provenance',
			);
		}
		return packet.path;
	});
	const artDirection = await loadArtDirection(
		repoRoot,
		options.artDirectionPath,
		options.fixture === true,
	);
	const loaded = await loadReviewPackets(repoRoot, packetPaths, artDirection);
	for (const [index, packet] of loaded.packets.entries()) {
		if (packet.sha256 !== template.source_packets[index].sha256) {
			throw new ReviewError('A source review packet changed after preparation');
		}
	}
	const expectedNamedCount = options.expectedNamedCount ?? NAMED_CAST_SIZE;
	const binding = validateWholeCastCandidates(
		loaded.candidates,
		expectedNamedCount,
	);
	if (canonicalJson(binding) !== canonicalJson(template.cast)) {
		throw new ReviewError(
			'Whole-cast candidate binding changed after preparation',
		);
	}
	validateCompletedChecklist(template, wholeCastChecklist);
	const decision = String(template.decision ?? '').toLowerCase();
	if (decision !== 'approved' && decision !== 'rejected') {
		throw new ReviewError('Whole-cast decision must be approved or rejected');
	}
	const reviewer = String(template.reviewer ?? '').trim();
	if (!reviewer) throw new ReviewError('Whole-cast reviewer is required');
	const notes = String(template.notes ?? '').trim();
	const failedChecks = Object.entries(template.checklist)
		.filter(([, passed]) => !passed)
		.map(([key]) => key);
	if (decision === 'approved' && failedChecks.length > 0) {
		throw new ReviewError(
			'Whole-cast approval requires every cast distinctiveness check',
		);
	}
	if (decision === 'rejected' && (!notes || failedChecks.length === 0)) {
		throw new ReviewError(
			'Whole-cast rejection requires notes and a failed checklist item',
		);
	}
	const recordBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-whole-cast-human-review-decision',
		source_packets: template.source_packets,
		cast: template.cast,
		decision,
		promotion_eligible: decision === 'approved',
		reviewer,
		reviewed_at: new Date().toISOString(),
		notes,
		checklist: template.checklist,
		source_template_path: portablePath(repoRoot, templatePath),
	};
	const record = { ...recordBase, review_id: computeReviewId(recordBase) };
	const recordPath = join(
		dirname(templatePath),
		'reviews',
		`${record.review_id}.json`,
	);
	await mkdir(dirname(recordPath), { recursive: true });
	await writeFile(recordPath, prettyJson(record), { flag: 'wx' });
	return { record, recordPath };
}

function validateCompletedChecklist(template, definition) {
	const expected = Object.keys(definition).toSorted();
	const actual = Object.keys(template.checklist ?? {}).toSorted();
	if (JSON.stringify(expected) !== JSON.stringify(actual)) {
		throw new ReviewError(
			'Review checklist keys do not match the asset contract',
		);
	}
	for (const key of expected) {
		if (typeof template.checklist[key] !== 'boolean') {
			throw new ReviewError(
				`Checklist item ${key} must be explicitly true or false`,
			);
		}
	}
}

export async function submitReviewDecision(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	if (!options.templatePath) {
		throw new ReviewError('decide requires --template');
	}
	const templatePath = resolvePath(repoRoot, options.templatePath);
	const template = (await loadJson(templatePath, 'review template')).value;
	if (
		template.schema_version !== 1 ||
		template.template_type !== 'notebook-person-art-human-review'
	) {
		throw new ReviewError(`${templatePath} is not a v1 review template`);
	}
	const loaded = await loadCandidate(repoRoot, template.candidate_receipt_path);
	if (template.candidate_receipt_sha256 !== loaded.receiptSha256) {
		throw new ReviewError(
			'Candidate receipt changed after the review packet was prepared',
		);
	}
	if (template.candidate_sha256 !== loaded.candidateSha256) {
		throw new ReviewError(
			'Candidate hash changed after the review packet was prepared',
		);
	}
	let bindings;
	if (loaded.receipt.asset.kind === 'pair') {
		const artDirection = await loadArtDirection(
			repoRoot,
			options.artDirectionPath,
			options.fixture === true,
		);
		bindings = identityBindings(artDirection, loaded.receipt.subject, {
			hairTopology: template.hair_topology,
			markerIdentity: template.marker_identity,
			label: 'Pair review template',
		});
	}
	const decision = String(template.decision ?? '').toLowerCase();
	if (decision !== 'approved' && decision !== 'rejected') {
		throw new ReviewError('Review decision must be approved or rejected');
	}
	const reviewer = String(template.reviewer ?? '').trim();
	if (!reviewer) throw new ReviewError('Review reviewer is required');
	const notes = String(template.notes ?? '').trim();
	const definition = checklistDefinition(loaded.receipt.asset.kind);
	validateCompletedChecklist(template, definition);
	const failedChecks = Object.entries(template.checklist)
		.filter(([, passed]) => !passed)
		.map(([key]) => key);
	if (decision === 'approved' && failedChecks.length > 0) {
		throw new ReviewError(
			`Approval requires every checklist item to pass; failed: ${failedChecks.join(', ')}`,
		);
	}
	if (decision === 'rejected' && (!notes || failedChecks.length === 0)) {
		throw new ReviewError(
			'Rejection requires notes and at least one failed checklist item',
		);
	}

	const objectRoot = dirname(loaded.receiptPath);
	const pointerPath = join(objectRoot, 'review.json');
	if (await fileExists(pointerPath)) {
		throw new ReviewError(
			`Candidate already has a review pointer at ${portablePath(repoRoot, pointerPath)}`,
		);
	}
	const reviewedAt = new Date().toISOString();
	const recordBase = {
		schema_version: 1,
		record_type: 'notebook-person-art-human-review-decision',
		candidate_receipt_path: portablePath(repoRoot, loaded.receiptPath),
		candidate_receipt_sha256: loaded.receiptSha256,
		candidate_sha256: loaded.candidateSha256,
		raw_sha256: loaded.receipt.artifact.raw_sha256,
		subject: loaded.receipt.subject,
		asset: loaded.receipt.asset,
		...(bindings
			? {
					hair_topology: bindings.hairTopology,
					marker_identity: bindings.markerIdentity,
				}
			: {}),
		decision,
		promotion_eligible: decision === 'approved',
		reviewer,
		reviewed_at: reviewedAt,
		notes,
		checklist: template.checklist,
		source_template_path: portablePath(repoRoot, templatePath),
	};
	const reviewId = computeReviewId(recordBase);
	const record = { ...recordBase, review_id: reviewId };
	const recordPath = join(
		loaded.reviewStorageRoot,
		'reviews',
		`${reviewId}.json`,
	);
	await mkdir(dirname(recordPath), { recursive: true });
	await writeFile(recordPath, prettyJson(record), { flag: 'wx' });
	const recordBytes = await readFile(recordPath);
	const pointer = {
		schema_version: 1,
		candidate_sha256: record.candidate_sha256,
		decision,
		promotion_eligible: record.promotion_eligible,
		record_path: portablePath(repoRoot, recordPath),
		record_sha256: sha256(recordBytes),
	};
	try {
		await writeFile(pointerPath, prettyJson(pointer), { flag: 'wx' });
	} catch (error) {
		if (error.code === 'EEXIST') {
			throw new ReviewError(
				'Another reviewer recorded a decision concurrently',
			);
		}
		throw error;
	}
	return { record, recordPath, pointer, pointerPath };
}

export async function readReviewStatus(options = {}) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	if (!options.receiptPath) throw new ReviewError('status requires --receipt');
	const loaded = await loadCandidate(repoRoot, options.receiptPath);
	const pointerPath = join(dirname(loaded.receiptPath), 'review.json');
	if (!(await fileExists(pointerPath))) {
		return {
			status: 'pending',
			candidate_sha256: loaded.candidateSha256,
		};
	}
	const pointer = (await loadJson(pointerPath, 'review pointer')).value;
	const recordPath = resolvePath(repoRoot, pointer.record_path);
	const recordFile = await loadJson(recordPath, 'review decision');
	if (recordFile.sha256 !== pointer.record_sha256) {
		throw new ReviewError('Review decision hash does not match review pointer');
	}
	if (
		pointer.candidate_sha256 !== loaded.candidateSha256 ||
		recordFile.value.candidate_sha256 !== loaded.candidateSha256
	) {
		throw new ReviewError('Review decision points to a different candidate');
	}
	return {
		status: pointer.decision,
		promotion_eligible: pointer.promotion_eligible,
		reviewer: recordFile.value.reviewer,
		reviewed_at: recordFile.value.reviewed_at,
		record_path: portablePath(repoRoot, recordPath),
		candidate_sha256: pointer.candidate_sha256,
	};
}

function cliOptions() {
	const parsed = parseArgs({
		options: {
			receipt: { type: 'string', multiple: true },
			packet: { type: 'string', multiple: true },
			template: { type: 'string' },
			output: { type: 'string' },
			'packet-id': { type: 'string' },
			'art-direction': { type: 'string' },
			fixture: { type: 'boolean' },
		},
		allowPositionals: true,
		strict: true,
	});
	const command = parsed.positionals[0];
	if (
		!['prepare', 'decide', 'status', 'prepare-cast', 'decide-cast'].includes(
			command,
		)
	) {
		throw new ReviewError(
			'Command must be prepare, decide, status, prepare-cast, or decide-cast',
		);
	}
	return {
		command,
		packetPaths: parsed.values.packet,
		receiptPaths: parsed.values.receipt,
		receiptPath: parsed.values.receipt?.[0],
		templatePath: parsed.values.template,
		outputDir: parsed.values.output,
		packetId: parsed.values['packet-id'],
		artDirectionPath: parsed.values['art-direction'],
		fixture: parsed.values.fixture,
	};
}

async function main() {
	const options = cliOptions();
	let result;
	if (options.command === 'prepare') {
		result = await prepareReviewPacket(options);
		console.log(
			prettyJson({
				packet_id: result.packetId,
				html: portablePath(defaultRepoRoot, result.htmlPath),
				templates: result.templatePaths.map((path) =>
					portablePath(defaultRepoRoot, path),
				),
			}).trimEnd(),
		);
	} else if (options.command === 'prepare-cast') {
		result = await prepareWholeCastReview(options);
		console.log(
			prettyJson({
				html: portablePath(defaultRepoRoot, result.htmlPath),
				template: portablePath(defaultRepoRoot, result.templatePath),
			}).trimEnd(),
		);
	} else if (options.command === 'decide-cast') {
		result = await submitWholeCastReviewDecision(options);
		console.log(
			prettyJson({
				decision: result.record.decision,
				promotion_eligible: result.record.promotion_eligible,
				record: portablePath(defaultRepoRoot, result.recordPath),
			}).trimEnd(),
		);
	} else if (options.command === 'decide') {
		result = await submitReviewDecision(options);
		console.log(
			prettyJson({
				decision: result.record.decision,
				promotion_eligible: result.record.promotion_eligible,
				record: portablePath(defaultRepoRoot, result.recordPath),
			}).trimEnd(),
		);
	} else {
		result = await readReviewStatus(options);
		console.log(prettyJson(result).trimEnd());
	}
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
