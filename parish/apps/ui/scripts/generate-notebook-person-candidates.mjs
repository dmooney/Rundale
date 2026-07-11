import { createHash, randomUUID } from 'node:crypto';
import {
	appendFile,
	mkdir,
	open,
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
} from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { PNG } from 'pngjs';

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(here, '..');
const defaultRepoRoot = resolve(uiRoot, '../../..');
const defaultArtRoot = join(uiRoot, 'art', 'notebook-person-art');
const defaultConfigPath = join(defaultArtRoot, 'generation-config-v1.json');
const defaultInputsPath = join(defaultArtRoot, 'npc-art-inputs-v1.json');

const sleep = (milliseconds) =>
	new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

class PipelineError extends Error {}

class ProviderError extends Error {
	constructor(message, { status = null, retryAfterMs = null } = {}) {
		super(message);
		this.status = status;
		this.retryAfterMs = retryAfterMs;
	}

	get retryable() {
		return (
			this.status === null ||
			this.status === 408 ||
			this.status === 409 ||
			this.status === 429 ||
			this.status >= 500
		);
	}
}

function canonicalJson(value) {
	if (value === null || typeof value !== 'object') {
		return JSON.stringify(value);
	}
	if (Array.isArray(value)) {
		return `[${value.map(canonicalJson).join(',')}]`;
	}
	const entries = Object.entries(value)
		.filter(([, child]) => child !== undefined)
		.sort(([left], [right]) => left.localeCompare(right));
	return `{${entries
		.map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
		.join(',')}}`;
}

function sha256(value) {
	return createHash('sha256').update(value).digest('hex');
}

function prettyJson(value) {
	return `${JSON.stringify(value, null, '\t')}\n`;
}

function resolvePath(repoRoot, configuredPath) {
	return isAbsolute(configuredPath)
		? configuredPath
		: resolve(repoRoot, configuredPath);
}

function receiptPath(repoRoot, path) {
	const local = relative(repoRoot, path);
	return local && !local.startsWith('..') ? local.replaceAll('\\', '/') : path;
}

function parsePositiveInteger(value, label) {
	const parsed = Number.parseInt(String(value), 10);
	if (!Number.isSafeInteger(parsed) || parsed <= 0) {
		throw new PipelineError(
			`${label} must be a positive integer; got ${value}`,
		);
	}
	return parsed;
}

function parseNonNegativeInteger(value, label) {
	const parsed = Number.parseInt(String(value), 10);
	if (!Number.isSafeInteger(parsed) || parsed < 0) {
		throw new PipelineError(
			`${label} must be a non-negative integer; got ${value}`,
		);
	}
	return parsed;
}

function parseKeyColor(hex) {
	if (!/^#[0-9a-f]{6}$/i.test(hex)) {
		throw new PipelineError(`raw_output.key_color must be #rrggbb; got ${hex}`);
	}
	return [
		Number.parseInt(hex.slice(1, 3), 16),
		Number.parseInt(hex.slice(3, 5), 16),
		Number.parseInt(hex.slice(5, 7), 16),
	];
}

function parseSize(value) {
	const match = /^(\d+)x(\d+)$/.exec(value);
	if (!match) {
		throw new PipelineError(
			`provider.request.size must be WIDTHxHEIGHT; got ${value}`,
		);
	}
	return {
		width: parsePositiveInteger(match[1], 'image width'),
		height: parsePositiveInteger(match[2], 'image height'),
	};
}

function validateConfig(config) {
	if (config.schema_version !== 1) {
		throw new PipelineError(
			`Unsupported generation config schema_version ${config.schema_version}`,
		);
	}
	if (config.provider?.adapter !== 'openai-images-edits-v1') {
		throw new PipelineError(
			`Unsupported provider adapter ${config.provider?.adapter ?? 'missing'}`,
		);
	}
	if (
		config.generation_mode !== undefined &&
		config.generation_mode !== 'paired-v1'
	) {
		throw new PipelineError(
			`Unsupported generation_mode ${config.generation_mode}`,
		);
	}
	for (const field of ['base_url', 'api_key_env', 'model', 'endpoint']) {
		if (!config.provider[field]) {
			throw new PipelineError(`provider.${field} is required`);
		}
	}
	if (config.provider.request?.output_format !== 'png') {
		throw new PipelineError(
			'The v1 candidate pipeline requires PNG provider output',
		);
	}
	if (
		!Array.isArray(config.reference_inputs) ||
		config.reference_inputs.length === 0
	) {
		throw new PipelineError('At least one reference input is required');
	}
	for (const reference of config.reference_inputs) {
		if (!reference.id || !reference.path || !reference.purpose) {
			throw new PipelineError(
				'Every reference input requires id, path, and purpose',
			);
		}
		if (
			reference.asset_kinds !== undefined &&
			(!Array.isArray(reference.asset_kinds) ||
				reference.asset_kinds.length === 0 ||
				reference.asset_kinds.some(
					(kind) => kind !== 'portrait' && kind !== 'marker' && kind !== 'pair',
				))
		) {
			throw new PipelineError(
				`reference_inputs.${reference.id}.asset_kinds must contain portrait, marker, and/or pair`,
			);
		}
	}
	if (config.approval?.auto_promote !== false) {
		throw new PipelineError('approval.auto_promote must remain false');
	}
	if (
		config.approval.generated_status !== 'candidate' ||
		config.approval.review_status !== 'pending'
	) {
		throw new PipelineError(
			'Generated assets must begin as candidate/pending, never approved',
		);
	}
	parseSize(config.provider.request.size);
	parseKeyColor(config.raw_output.key_color);
	if (!config.raw_output.postprocess_revision) {
		throw new PipelineError('raw_output.postprocess_revision is required');
	}
	const framingNormalization = config.raw_output.framing_normalization;
	if (framingNormalization?.enabled) {
		if (framingNormalization.algorithm !== 'premultiplied-bilinear-v1') {
			throw new PipelineError(
				`Unsupported framing normalization algorithm ${framingNormalization.algorithm}`,
			);
		}
		if (
			typeof framingNormalization.headroom_fraction !== 'number' ||
			framingNormalization.headroom_fraction < 0 ||
			framingNormalization.headroom_fraction >= 0.1
		) {
			throw new PipelineError(
				'raw_output.framing_normalization.headroom_fraction must be between 0 and 0.1',
			);
		}
		parsePositiveInteger(
			framingNormalization.min_axis_pixels,
			'raw_output.framing_normalization.min_axis_pixels',
		);
	}
	parseKeyColor(config.raw_output.portrait_ink_color);
	if (config.generation_mode === 'paired-v1') {
		if (!config.raw_output.pair_contract) {
			throw new PipelineError(
				'raw_output.pair_contract is required for paired generation',
			);
		}
		const requestSize = parseSize(config.provider.request.size);
		const cellSize = parseSize(config.paired_output?.cell_size);
		if (
			requestSize.width !== cellSize.width * 2 ||
			requestSize.height !== cellSize.height ||
			config.paired_output?.portrait?.x !== 0 ||
			config.paired_output?.portrait?.y !== 0 ||
			config.paired_output?.marker?.x !== cellSize.width ||
			config.paired_output?.marker?.y !== 0
		) {
			throw new PipelineError(
				'paired_output must define two exact horizontal cells covering the provider canvas',
			);
		}
	}
	parsePositiveInteger(config.candidate_count, 'candidate_count');
	parsePositiveInteger(
		config.rate_limit.requests_per_minute,
		'rate_limit.requests_per_minute',
	);
	parsePositiveInteger(
		config.rate_limit.max_concurrency,
		'rate_limit.max_concurrency',
	);
	parsePositiveInteger(config.retry.max_attempts, 'retry.max_attempts');
	parsePositiveInteger(
		config.retry.request_timeout_ms,
		'retry.request_timeout_ms',
	);
}

function validateInputs(inputs) {
	if (inputs.schema_version !== 1) {
		throw new PipelineError(
			`Unsupported NPC art-input schema_version ${inputs.schema_version}`,
		);
	}
	if (!Array.isArray(inputs.npcs) || inputs.npcs.length === 0) {
		throw new PipelineError('NPC art-input dataset has no NPC records');
	}
	if (
		!inputs.fallback?.pair_prompt ||
		!inputs.fallback?.portrait_prompt ||
		!inputs.fallback?.marker_prompt
	) {
		throw new PipelineError(
			'NPC art-input dataset is missing fallback prompts',
		);
	}
	const seen = new Set();
	for (const npc of inputs.npcs) {
		if (!Number.isInteger(npc.npc_id) || !npc.name) {
			throw new PipelineError('Every NPC art input needs npc_id and name');
		}
		if (seen.has(npc.npc_id)) {
			throw new PipelineError(`Duplicate NPC art input id ${npc.npc_id}`);
		}
		seen.add(npc.npc_id);
		if (!npc.pair_prompt || !npc.portrait_prompt || !npc.marker_prompt) {
			throw new PipelineError(`NPC ${npc.npc_id} is missing generated prompts`);
		}
	}
}

async function loadJsonWithHash(path, label) {
	const bytes = await readFile(path);
	let value;
	try {
		value = JSON.parse(bytes.toString('utf8'));
	} catch (error) {
		throw new PipelineError(
			`Could not parse ${label} ${path}: ${error.message}`,
		);
	}
	return { value, sha256: sha256(bytes) };
}

async function loadPipeline(options) {
	const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot);
	const configPath = resolvePath(
		repoRoot,
		options.configPath ?? defaultConfigPath,
	);
	const inputsPath = resolvePath(
		repoRoot,
		options.inputsPath ?? defaultInputsPath,
	);
	const configFile = await loadJsonWithHash(configPath, 'generation config');
	const inputsFile = await loadJsonWithHash(inputsPath, 'NPC art inputs');
	validateConfig(configFile.value);
	validateInputs(inputsFile.value);

	const references = [];
	for (const reference of configFile.value.reference_inputs) {
		const path = resolvePath(repoRoot, reference.path);
		const buffer = await readFile(path);
		references.push({
			...reference,
			path,
			buffer,
			sha256: sha256(buffer),
		});
	}

	return {
		repoRoot,
		configPath,
		config: configFile.value,
		configSha256: configFile.sha256,
		inputsPath,
		inputs: inputsFile.value,
		inputsSha256: inputsFile.sha256,
		references,
	};
}

function selectedAssetKinds(asset, generationMode) {
	if (generationMode === 'paired-v1') {
		if (!asset || asset === 'all' || asset === 'pair') return ['pair'];
		throw new PipelineError(
			`Paired generation always creates portrait and marker together; use --asset pair, not ${asset}`,
		);
	}
	if (!asset || asset === 'all') return ['portrait', 'marker'];
	if (asset === 'portrait' || asset === 'marker') return [asset];
	throw new PipelineError(
		`--asset must be all, portrait, marker, or pair; got ${asset}`,
	);
}

function referencesForAsset(references, assetKind) {
	return references.filter(
		(reference) =>
			!reference.asset_kinds || reference.asset_kinds.includes(assetKind),
	);
}

function providerPrompt(basePrompt, job, pipeline, references) {
	const contract =
		job.assetKind === 'pair'
			? `${pipeline.config.raw_output.pair_contract}\nLeft-cell portrait contract: ${pipeline.config.raw_output.portrait_contract}\nRight-cell marker contract: ${pipeline.config.raw_output.marker_contract}`
			: job.assetKind === 'portrait'
				? pipeline.config.raw_output.portrait_contract
				: pipeline.config.raw_output.marker_contract;
	const referenceRules = references
		.map(
			(reference) => `Attached reference ${reference.id}: ${reference.purpose}`,
		)
		.join('\n');
	const finalInvariant =
		job.assetKind === 'pair'
			? 'Final invariant check: the left portrait and right marker must unmistakably be the same person. Keep each rendering wholly inside its assigned square cell, with a flat keyed center boundary and no labels or dividers. In the left cell, every pixel that is not a sparse dark ink stroke must remain flat #ff00ff, including uninked areas inside the face, hair, neck, clothing, and prop; never add white, cream, parchment, skin-tone, gray, or other fill.'
			: job.assetKind === 'portrait'
				? 'Final invariant check: this must read as a quick player-drawn notebook sketch with sparse contour lines and almost no shading, never as a finished illustration. Most of the keyed canvas and most interior areas of the figure must remain visibly unpainted.'
				: 'Final invariant check: preserve the small, readable in-world marker silhouette and restrained painted-world treatment.';
	const depictionCount =
		job.assetKind === 'pair'
			? 'Create exactly two depictions of one character, one in each assigned cell.'
			: 'Create one character only.';
	return `${basePrompt}\n\nProvider rendering contract (controls the raw API output): ${contract}\nReference roles:\n${referenceRules}\n${depictionCount} Candidate index ${job.candidateIndex}; do not add a contact sheet, variants, labels, or surrounding UI.\n${finalInvariant}`;
}

function buildJobs(pipeline, options = {}) {
	const candidateCount = parsePositiveInteger(
		options.candidateCount ?? pipeline.config.candidate_count,
		'candidate count',
	);
	const assetKinds = selectedAssetKinds(
		options.asset,
		pipeline.config.generation_mode,
	);
	const requestedIds = new Set(
		(options.npcIds ?? []).map((id) => parsePositiveInteger(id, '--npc-id')),
	);
	const availableIds = new Set(pipeline.inputs.npcs.map((npc) => npc.npc_id));
	for (const requestedId of requestedIds) {
		if (!availableIds.has(requestedId)) {
			throw new PipelineError(`No NPC art input exists for id ${requestedId}`);
		}
	}

	const subjects = pipeline.inputs.npcs
		.filter((npc) => requestedIds.size === 0 || requestedIds.has(npc.npc_id))
		.map((npc) => ({
			subjectKind: 'npc',
			npcId: npc.npc_id,
			name: npc.name,
			record: npc,
		}));
	const includeFallback =
		options.includeFallback ?? pipeline.config.include_fallback;
	if (includeFallback) {
		subjects.push({
			subjectKind: 'fallback',
			npcId: null,
			name: 'Unknown parish neighbour',
			record: pipeline.inputs.fallback,
		});
	}

	const jobs = [];
	for (const subject of subjects) {
		for (const assetKind of assetKinds) {
			const references = referencesForAsset(pipeline.references, assetKind);
			if (references.length === 0) {
				throw new PipelineError(
					`No configured reference input applies to ${assetKind} assets`,
				);
			}
			for (
				let candidateIndex = 1;
				candidateIndex <= candidateCount;
				candidateIndex += 1
			) {
				const basePrompt = subject.record[`${assetKind}_prompt`];
				const draft = { assetKind, candidateIndex };
				const prompt = providerPrompt(basePrompt, draft, pipeline, references);
				const inputRecordSha256 = sha256(canonicalJson(subject.record));
				const identity = {
					schema_version: 1,
					pipeline_revision: pipeline.config.pipeline_revision,
					provider: {
						id: pipeline.config.provider.id,
						adapter: pipeline.config.provider.adapter,
						model: pipeline.config.provider.model,
						request: pipeline.config.provider.request,
					},
					raw_output: pipeline.config.raw_output,
					validation: pipeline.config.validation,
					reference_inputs: references.map((reference) => ({
						id: reference.id,
						sha256: reference.sha256,
					})),
					subject_kind: subject.subjectKind,
					npc_id: subject.npcId,
					input_record_sha256: inputRecordSha256,
					asset_kind: assetKind,
					candidate_index: candidateIndex,
					prompt_sha256: sha256(prompt),
				};
				const jobId = sha256(canonicalJson(identity));
				jobs.push({
					jobId,
					identity,
					subject,
					assetKind,
					candidateIndex,
					prompt,
					promptSha256: identity.prompt_sha256,
					inputRecordSha256,
					references,
				});
			}
		}
	}

	jobs.sort((left, right) => left.jobId.localeCompare(right.jobId));
	const shardCount = parsePositiveInteger(
		options.shardCount ?? 1,
		'--shard-count',
	);
	const shardIndex = parseNonNegativeInteger(
		options.shardIndex ?? 0,
		'--shard-index',
	);
	if (shardIndex >= shardCount) {
		throw new PipelineError(
			`--shard-index ${shardIndex} must be less than --shard-count ${shardCount}`,
		);
	}
	const shardedJobs = jobs.filter(
		(job) =>
			Number.parseInt(job.jobId.slice(0, 8), 16) % shardCount === shardIndex,
	);
	return {
		jobs: shardedJobs,
		unshardedJobCount: jobs.length,
		shardIndex,
		shardCount,
	};
}

function objectPaths(outputRoot, job) {
	const root = join(outputRoot, 'objects', job.jobId.slice(0, 2), job.jobId);
	return {
		root,
		lock: join(root, 'generation.lock'),
		prompt: join(root, 'prompt.txt'),
		inputRecord: join(root, 'input-record.json'),
		receipt: join(root, 'receipt.json'),
	};
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

async function fileExists(path) {
	try {
		await stat(path);
		return true;
	} catch (error) {
		if (error.code === 'ENOENT') return false;
		throw error;
	}
}

async function existingReceipt(repoRoot, paths, job) {
	if (!(await fileExists(paths.receipt))) return null;
	try {
		const receipt = JSON.parse(await readFile(paths.receipt, 'utf8'));
		if (
			receipt.job_id !== job.jobId ||
			receipt.status !== 'candidate' ||
			receipt.review?.status !== 'pending' ||
			!receipt.artifact?.raw_path
		) {
			return null;
		}
		const artifacts = [
			{
				path: receipt.artifact.raw_path,
				hash: receipt.artifact.raw_sha256,
			},
		];
		if (job.assetKind === 'pair') {
			if (
				receipt.receipt_type !== 'notebook-person-art-pair-candidate' ||
				receipt.asset?.kind !== 'pair'
			) {
				return null;
			}
			for (const kind of ['portrait', 'marker']) {
				const child = receipt.artifact.children?.[kind];
				if (
					!child?.raw_path ||
					!child.raw_sha256 ||
					!child.candidate_path ||
					!child.candidate_sha256
				) {
					return null;
				}
				artifacts.push(
					{ path: child.raw_path, hash: child.raw_sha256 },
					{ path: child.candidate_path, hash: child.candidate_sha256 },
				);
			}
		} else {
			if (!receipt.artifact.candidate_path) return null;
			artifacts.push({
				path: receipt.artifact.candidate_path,
				hash: receipt.artifact.candidate_sha256,
			});
		}
		for (const artifact of artifacts) {
			const path = resolvePath(repoRoot, artifact.path);
			if (!(await fileExists(path))) return null;
			if (sha256(await readFile(path)) !== artifact.hash) return null;
		}
		return {
			receipt,
			receiptPath: receiptPath(repoRoot, paths.receipt),
		};
	} catch {
		return null;
	}
}

function pixelDistance(r, g, b, key) {
	return Math.hypot(r - key[0], g - key[1], b - key[2]);
}

function validationForAsset(config, assetKind) {
	return {
		...config.validation,
		...(config.validation.asset_contracts?.[assetKind] ?? {}),
	};
}

function pixelLuminance(r, g, b) {
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function pixelChroma(r, g, b) {
	return Math.max(r, g, b) - Math.min(r, g, b);
}

function isMagentaKeySpill(r, g, b, key, validation) {
	if (!(key[0] > 200 && key[2] > 200 && key[1] < 80)) return false;
	const chromaMinimum = validation.key_spill_chroma_min ?? 24;
	const balanceMaximum = validation.key_spill_balance_max ?? 40;
	return (
		Math.min(r, b) - g > chromaMinimum && Math.abs(r - b) <= balanceMaximum
	);
}

function expectedSizeForAsset(config, assetKind) {
	if (
		config.generation_mode === 'paired-v1' &&
		(assetKind === 'portrait' || assetKind === 'marker')
	) {
		return parseSize(config.paired_output.cell_size);
	}
	return parseSize(config.provider.request.size);
}

function inspectRawPng(buffer, config, assetKind, options = {}) {
	let png;
	try {
		png = PNG.sync.read(buffer, { checkCRC: true });
	} catch (error) {
		throw new PipelineError(
			`Provider output is not a valid PNG: ${error.message}`,
		);
	}
	const expected = expectedSizeForAsset(config, assetKind);
	if (png.width !== expected.width || png.height !== expected.height) {
		throw new PipelineError(
			`Provider output is ${png.width}x${png.height}; expected ${expected.width}x${expected.height}`,
		);
	}

	const validation = validationForAsset(config, assetKind);
	const key = parseKeyColor(config.raw_output.key_color);
	const threshold = validation.key_distance;
	const total = png.width * png.height;
	let keyPixels = 0;
	let subjectPixels = 0;
	let inkPixels = 0;
	let lightSubjectPixels = 0;
	let coloredSubjectPixels = 0;
	const coloredSubjectMask = new Uint8Array(total);
	const subjectRows = new Uint32Array(png.height);
	const subjectColumns = new Uint32Array(png.width);
	let minX = png.width;
	let minY = png.height;
	let maxX = -1;
	let maxY = -1;
	let inkMinX = png.width;
	let inkMinY = png.height;
	let inkMaxX = -1;
	let inkMaxY = -1;
	const colorBuckets = new Set();
	const inkLuminanceMax = validation.ink_luminance_max ?? 192;
	const inkChromaMax = validation.ink_chroma_max ?? 72;
	for (let y = 0; y < png.height; y += 1) {
		for (let x = 0; x < png.width; x += 1) {
			const pixelIndex = y * png.width + x;
			const offset = pixelIndex * 4;
			const r = png.data[offset];
			const g = png.data[offset + 1];
			const b = png.data[offset + 2];
			const alpha = png.data[offset + 3];
			const keyDistance = pixelDistance(r, g, b, key);
			if (alpha > 16 && keyDistance <= threshold) {
				keyPixels += 1;
			} else if (alpha > 16) {
				subjectPixels += 1;
				subjectRows[y] += 1;
				subjectColumns[x] += 1;
				minX = Math.min(minX, x);
				minY = Math.min(minY, y);
				maxX = Math.max(maxX, x);
				maxY = Math.max(maxY, y);
				if (colorBuckets.size < 4096) {
					colorBuckets.add(`${r >> 4}:${g >> 4}:${b >> 4}:${alpha >> 4}`);
				}
				const luminance = pixelLuminance(r, g, b);
				const chroma = pixelChroma(r, g, b);
				if (luminance <= inkLuminanceMax && chroma <= inkChromaMax) {
					inkPixels += 1;
					inkMinX = Math.min(inkMinX, x);
					inkMinY = Math.min(inkMinY, y);
					inkMaxX = Math.max(inkMaxX, x);
					inkMaxY = Math.max(inkMaxY, y);
				} else if (
					chroma > inkChromaMax &&
					keyDistance >= validation.key_feather_distance
				) {
					coloredSubjectPixels += 1;
					coloredSubjectMask[pixelIndex] = 1;
				} else {
					lightSubjectPixels += 1;
				}
			}
		}
	}
	let coloredFillPixels = 0;
	// Solid fills survive a one-pixel erosion; thin sepia/key antialiasing does not.
	for (let y = 1; y < png.height - 1; y += 1) {
		for (let x = 1; x < png.width - 1; x += 1) {
			const pixelIndex = y * png.width + x;
			if (!coloredSubjectMask[pixelIndex]) continue;
			let solidInterior = true;
			for (let offsetY = -1; offsetY <= 1 && solidInterior; offsetY += 1) {
				for (let offsetX = -1; offsetX <= 1; offsetX += 1) {
					if (!coloredSubjectMask[pixelIndex + offsetY * png.width + offsetX]) {
						solidInterior = false;
						break;
					}
				}
			}
			if (solidInterior) coloredFillPixels += 1;
		}
	}
	const cornerThreshold = validation.key_feather_distance;
	const corners = [
		[0, 0],
		[png.width - 1, 0],
		[0, png.height - 1],
		[png.width - 1, png.height - 1],
	];
	const keyedCorners = corners.filter(([x, y]) => {
		const offset = (y * png.width + x) * 4;
		return (
			pixelDistance(
				png.data[offset],
				png.data[offset + 1],
				png.data[offset + 2],
				key,
			) <= cornerThreshold
		);
	}).length;
	const inkBounds =
		inkMaxX < inkMinX
			? null
			: {
					x: inkMinX,
					y: inkMinY,
					width: inkMaxX - inkMinX + 1,
					height: inkMaxY - inkMinY + 1,
				};
	const inkBoundsArea = inkBounds ? inkBounds.width * inkBounds.height : 0;
	const minAxisPixels =
		config.raw_output.framing_normalization?.min_axis_pixels ?? 1;
	const significantColumns = Array.from(subjectColumns.entries()).filter(
		([, count]) => count >= minAxisPixels,
	);
	const significantRows = Array.from(subjectRows.entries()).filter(
		([, count]) => count >= minAxisPixels,
	);
	const boundsMinX = significantColumns[0]?.[0] ?? minX;
	const boundsMaxX = significantColumns.at(-1)?.[0] ?? maxX;
	const boundsMinY = significantRows[0]?.[0] ?? minY;
	const boundsMaxY = significantRows.at(-1)?.[0] ?? maxY;
	const subjectBounds =
		maxX < minX
			? null
			: {
					x: boundsMinX,
					y: boundsMinY,
					width: boundsMaxX - boundsMinX + 1,
					height: boundsMaxY - boundsMinY + 1,
				};
	const subjectMargin = subjectBounds
		? Math.min(
				subjectBounds.x,
				subjectBounds.y,
				png.width - (subjectBounds.x + subjectBounds.width),
				png.height - (subjectBounds.y + subjectBounds.height),
			) / Math.min(png.width, png.height)
		: 0;
	const stats = {
		width: png.width,
		height: png.height,
		key_fraction: keyPixels / total,
		subject_fraction: subjectPixels / total,
		ink_fraction: inkPixels / total,
		ink_fill_fraction: inkBoundsArea === 0 ? 0 : inkPixels / inkBoundsArea,
		light_subject_fraction: lightSubjectPixels / total,
		colored_subject_fraction: coloredSubjectPixels / total,
		colored_fill_fraction: coloredFillPixels / total,
		subject_color_buckets: colorBuckets.size,
		keyed_corners: keyedCorners,
		corner_key_distance: cornerThreshold,
		bounds_axis_min_pixels: minAxisPixels,
		subject_bounds: subjectBounds,
		subject_bounds_height_fraction: subjectBounds
			? subjectBounds.height / png.height
			: 0,
		subject_bounds_width_fraction: subjectBounds
			? subjectBounds.width / png.width
			: 0,
		subject_margin_fraction: subjectMargin,
		ink_bounds: inkBounds,
		ink_bounds_height_fraction: inkBounds ? inkBounds.height / png.height : 0,
	};
	if (stats.key_fraction < validation.min_key_fraction) {
		throw new PipelineError(
			`Provider output key coverage ${stats.key_fraction.toFixed(3)} is below ${validation.min_key_fraction}`,
		);
	}
	if (stats.key_fraction > validation.max_key_fraction) {
		throw new PipelineError(
			`Provider output is blank/degenerate: key coverage ${stats.key_fraction.toFixed(3)} exceeds ${validation.max_key_fraction}`,
		);
	}
	if (
		stats.subject_fraction < validation.min_subject_fraction ||
		stats.subject_fraction > validation.max_subject_fraction
	) {
		throw new PipelineError(
			`Provider subject coverage ${stats.subject_fraction.toFixed(3)} is outside ${validation.min_subject_fraction}-${validation.max_subject_fraction}`,
		);
	}
	if (
		validation.min_subject_bounds_height_fraction !== undefined &&
		stats.subject_bounds_height_fraction <
			validation.min_subject_bounds_height_fraction
	) {
		throw new PipelineError(
			`Provider subject height ${stats.subject_bounds_height_fraction.toFixed(3)} is below ${validation.min_subject_bounds_height_fraction}`,
		);
	}
	if (
		!options.allowOversizedFraming &&
		validation.max_subject_bounds_height_fraction !== undefined &&
		stats.subject_bounds_height_fraction >
			validation.max_subject_bounds_height_fraction
	) {
		throw new PipelineError(
			`Provider subject height ${stats.subject_bounds_height_fraction.toFixed(3)} exceeds ${validation.max_subject_bounds_height_fraction}`,
		);
	}
	if (
		!options.allowOversizedFraming &&
		validation.max_subject_bounds_width_fraction !== undefined &&
		stats.subject_bounds_width_fraction >
			validation.max_subject_bounds_width_fraction
	) {
		throw new PipelineError(
			`Provider subject width ${stats.subject_bounds_width_fraction.toFixed(3)} exceeds ${validation.max_subject_bounds_width_fraction}`,
		);
	}
	if (
		validation.min_subject_margin_fraction !== undefined &&
		stats.subject_margin_fraction < validation.min_subject_margin_fraction
	) {
		throw new PipelineError(
			`Provider subject margin ${stats.subject_margin_fraction.toFixed(3)} is below ${validation.min_subject_margin_fraction}; cropped or cell-boundary content detected`,
		);
	}
	if (stats.subject_color_buckets < validation.min_subject_color_buckets) {
		throw new PipelineError(
			`Provider output has only ${stats.subject_color_buckets} subject color buckets and is degenerate`,
		);
	}
	if (validation.require_keyed_corners && stats.keyed_corners !== 4) {
		throw new PipelineError(
			`Provider output has ${stats.keyed_corners}/4 keyed corners; isolated figure required`,
		);
	}
	if (
		validation.min_ink_bounds_height_fraction !== undefined &&
		stats.ink_bounds_height_fraction < validation.min_ink_bounds_height_fraction
	) {
		throw new PipelineError(
			`Provider inked drawing height ${stats.ink_bounds_height_fraction.toFixed(3)} is below ${validation.min_ink_bounds_height_fraction}`,
		);
	}
	if (
		!options.allowOversizedFraming &&
		validation.max_ink_bounds_height_fraction !== undefined &&
		stats.ink_bounds_height_fraction > validation.max_ink_bounds_height_fraction
	) {
		throw new PipelineError(
			`Provider inked drawing height ${stats.ink_bounds_height_fraction.toFixed(3)} exceeds ${validation.max_ink_bounds_height_fraction}`,
		);
	}
	for (const [field, limit, label] of [
		['ink_fraction', validation.max_ink_fraction, 'dark ink coverage'],
		[
			'ink_fill_fraction',
			validation.max_ink_fill_fraction,
			'ink density inside the drawing bounds',
		],
		[
			'light_subject_fraction',
			validation.max_light_subject_fraction,
			'light fill coverage',
		],
		[
			'colored_subject_fraction',
			validation.max_colored_subject_fraction,
			'colored subject coverage',
		],
		[
			'colored_fill_fraction',
			validation.max_colored_fill_fraction,
			'solid colored fill coverage',
		],
	]) {
		if (limit !== undefined && stats[field] > limit) {
			throw new PipelineError(
				`Provider ${label} ${stats[field].toFixed(3)} exceeds ${limit}`,
			);
		}
	}
	return { png, stats };
}

function chromaKeyCandidate(rawPng, config, assetKind) {
	const png = new PNG({ width: rawPng.width, height: rawPng.height });
	rawPng.data.copy(png.data);
	const validation = validationForAsset(config, assetKind);
	const key = parseKeyColor(config.raw_output.key_color);
	const hard = validation.key_distance;
	const feather = validation.key_feather_distance;
	const normalizedInk =
		assetKind === 'portrait'
			? parseKeyColor(config.raw_output.portrait_ink_color)
			: null;
	let transparentPixels = 0;
	let visiblePixels = 0;
	let residualKeySpillPixels = 0;
	for (let offset = 0; offset < png.data.length; offset += 4) {
		const r = png.data[offset];
		const g = png.data[offset + 1];
		const b = png.data[offset + 2];
		const alpha = png.data[offset + 3];
		const distance = pixelDistance(r, g, b, key);
		let nextAlpha = alpha;
		if (distance <= hard) nextAlpha = 0;
		else if (distance < feather) {
			nextAlpha = Math.round(alpha * ((distance - hard) / (feather - hard)));
		}
		if (nextAlpha < 255 && key[0] > 200 && key[2] > 200 && key[1] < 80) {
			const maxMagenta = Math.max(g + 72, 96);
			png.data[offset] = Math.min(r, maxMagenta);
			png.data[offset + 2] = Math.min(b, maxMagenta);
		}
		if (
			nextAlpha > 0 &&
			isMagentaKeySpill(
				png.data[offset],
				png.data[offset + 1],
				png.data[offset + 2],
				key,
				validation,
			)
		) {
			const neutral = Math.round(pixelLuminance(r, g, b));
			png.data[offset] = neutral;
			png.data[offset + 1] = neutral;
			png.data[offset + 2] = Math.max(0, neutral - 6);
		}
		if (normalizedInk && nextAlpha > 0) {
			png.data[offset] = normalizedInk[0];
			png.data[offset + 1] = normalizedInk[1];
			png.data[offset + 2] = normalizedInk[2];
		}
		png.data[offset + 3] = nextAlpha;
		if (nextAlpha < 16) transparentPixels += 1;
		if (nextAlpha > 32) {
			visiblePixels += 1;
			if (
				isMagentaKeySpill(
					png.data[offset],
					png.data[offset + 1],
					png.data[offset + 2],
					key,
					validation,
				)
			) {
				residualKeySpillPixels += 1;
			}
		}
	}
	const total = png.width * png.height;
	if (transparentPixels / total < validation.min_key_fraction * 0.9) {
		throw new PipelineError(
			'Chroma-key output retained too much provider background',
		);
	}
	if (visiblePixels / total < validation.min_subject_fraction * 0.8) {
		throw new PipelineError(
			'Chroma-key output removed the subject or is blank',
		);
	}
	const residualKeySpillFraction =
		visiblePixels === 0 ? 0 : residualKeySpillPixels / visiblePixels;
	if (
		validation.max_residual_key_spill_fraction !== undefined &&
		residualKeySpillFraction > validation.max_residual_key_spill_fraction
	) {
		throw new PipelineError(
			`Chroma-key output retained ${(residualKeySpillFraction * 100).toFixed(2)}% magenta spill among visible pixels; maximum is ${(validation.max_residual_key_spill_fraction * 100).toFixed(2)}%`,
		);
	}
	return {
		buffer: PNG.sync.write(png, { colorType: 6 }),
		stats: {
			transparent_fraction: transparentPixels / total,
			visible_fraction: visiblePixels / total,
			residual_key_spill_fraction: residualKeySpillFraction,
			normalized_ink_color: normalizedInk
				? config.raw_output.portrait_ink_color
				: null,
		},
	};
}

function alphaBounds(png, threshold = 32) {
	let minX = png.width;
	let minY = png.height;
	let maxX = -1;
	let maxY = -1;
	for (let y = 0; y < png.height; y += 1) {
		for (let x = 0; x < png.width; x += 1) {
			const alpha = png.data[(y * png.width + x) * 4 + 3];
			if (alpha <= threshold) continue;
			minX = Math.min(minX, x);
			minY = Math.min(minY, y);
			maxX = Math.max(maxX, x);
			maxY = Math.max(maxY, y);
		}
	}
	if (maxX < minX) return null;
	return {
		x: minX,
		y: minY,
		width: maxX - minX + 1,
		height: maxY - minY + 1,
	};
}

function framingStats(png) {
	const bounds = alphaBounds(png);
	if (!bounds) {
		throw new PipelineError('Transparent candidate is blank after key removal');
	}
	const margin =
		Math.min(
			bounds.x,
			bounds.y,
			png.width - (bounds.x + bounds.width),
			png.height - (bounds.y + bounds.height),
		) / Math.min(png.width, png.height);
	return {
		bounds,
		height_fraction: bounds.height / png.height,
		width_fraction: bounds.width / png.width,
		margin_fraction: margin,
	};
}

function bilinearPremultipliedSample(png, x, y) {
	const x0 = Math.max(0, Math.min(png.width - 1, Math.floor(x)));
	const y0 = Math.max(0, Math.min(png.height - 1, Math.floor(y)));
	const x1 = Math.max(0, Math.min(png.width - 1, x0 + 1));
	const y1 = Math.max(0, Math.min(png.height - 1, y0 + 1));
	const tx = Math.max(0, Math.min(1, x - Math.floor(x)));
	const ty = Math.max(0, Math.min(1, y - Math.floor(y)));
	const samples = [
		[x0, y0, (1 - tx) * (1 - ty)],
		[x1, y0, tx * (1 - ty)],
		[x0, y1, (1 - tx) * ty],
		[x1, y1, tx * ty],
	];
	let alpha = 0;
	let red = 0;
	let green = 0;
	let blue = 0;
	for (const [sampleX, sampleY, weight] of samples) {
		const offset = (sampleY * png.width + sampleX) * 4;
		const sampleAlpha = png.data[offset + 3] / 255;
		const weightedAlpha = sampleAlpha * weight;
		alpha += weightedAlpha;
		red += png.data[offset] * weightedAlpha;
		green += png.data[offset + 1] * weightedAlpha;
		blue += png.data[offset + 2] * weightedAlpha;
	}
	if (alpha <= 0) return [0, 0, 0, 0];
	return [
		Math.round(red / alpha),
		Math.round(green / alpha),
		Math.round(blue / alpha),
		Math.round(alpha * 255),
	];
}

function resizeCandidateSubject(png, bounds, scale) {
	const targetWidth = Math.max(1, Math.floor(bounds.width * scale));
	const targetHeight = Math.max(1, Math.floor(bounds.height * scale));
	const targetX = Math.floor((png.width - targetWidth) / 2);
	const targetY = Math.floor((png.height - targetHeight) / 2);
	const resized = new PNG({ width: png.width, height: png.height });
	resized.data.fill(0);
	for (let y = 0; y < targetHeight; y += 1) {
		const sourceY = bounds.y + ((y + 0.5) * bounds.height) / targetHeight - 0.5;
		for (let x = 0; x < targetWidth; x += 1) {
			const sourceX = bounds.x + ((x + 0.5) * bounds.width) / targetWidth - 0.5;
			const pixel = bilinearPremultipliedSample(png, sourceX, sourceY);
			const offset = ((targetY + y) * png.width + targetX + x) * 4;
			resized.data[offset] = pixel[0];
			resized.data[offset + 1] = pixel[1];
			resized.data[offset + 2] = pixel[2];
			resized.data[offset + 3] = pixel[3];
		}
	}
	return resized;
}

function candidatePixelStats(png, config, assetKind) {
	const validation = validationForAsset(config, assetKind);
	const key = parseKeyColor(config.raw_output.key_color);
	let transparentPixels = 0;
	let visiblePixels = 0;
	let residualKeySpillPixels = 0;
	for (let offset = 0; offset < png.data.length; offset += 4) {
		const alpha = png.data[offset + 3];
		if (alpha < 16) transparentPixels += 1;
		if (alpha <= 32) continue;
		visiblePixels += 1;
		if (
			isMagentaKeySpill(
				png.data[offset],
				png.data[offset + 1],
				png.data[offset + 2],
				key,
				validation,
			)
		) {
			residualKeySpillPixels += 1;
		}
	}
	const total = png.width * png.height;
	const residualKeySpillFraction =
		visiblePixels === 0 ? 0 : residualKeySpillPixels / visiblePixels;
	if (transparentPixels / total < validation.min_key_fraction * 0.9) {
		throw new PipelineError(
			'Framing normalization retained too much provider background',
		);
	}
	if (visiblePixels / total < validation.min_subject_fraction * 0.8) {
		throw new PipelineError(
			'Framing normalization removed the subject or produced a blank candidate',
		);
	}
	if (
		validation.max_residual_key_spill_fraction !== undefined &&
		residualKeySpillFraction > validation.max_residual_key_spill_fraction
	) {
		throw new PipelineError(
			`Framing normalization retained ${(residualKeySpillFraction * 100).toFixed(2)}% magenta spill among visible pixels`,
		);
	}
	return {
		transparent_fraction: transparentPixels / total,
		visible_fraction: visiblePixels / total,
		residual_key_spill_fraction: residualKeySpillFraction,
		normalized_ink_color:
			assetKind === 'portrait' ? config.raw_output.portrait_ink_color : null,
	};
}

function framingHeightContract(validation, assetKind) {
	if (assetKind === 'portrait') {
		return {
			minimum: validation.min_ink_bounds_height_fraction,
			maximum: validation.max_ink_bounds_height_fraction,
		};
	}
	return {
		minimum: validation.min_subject_bounds_height_fraction,
		maximum: validation.max_subject_bounds_height_fraction,
	};
}

function validateCandidateFraming(stats, validation, assetKind) {
	const height = framingHeightContract(validation, assetKind);
	if (height.minimum !== undefined && stats.height_fraction < height.minimum) {
		throw new PipelineError(
			`Normalized ${assetKind} height ${stats.height_fraction.toFixed(3)} is below ${height.minimum}`,
		);
	}
	if (height.maximum !== undefined && stats.height_fraction > height.maximum) {
		throw new PipelineError(
			`Normalized ${assetKind} height ${stats.height_fraction.toFixed(3)} exceeds ${height.maximum}`,
		);
	}
	if (
		validation.max_subject_bounds_width_fraction !== undefined &&
		stats.width_fraction > validation.max_subject_bounds_width_fraction
	) {
		throw new PipelineError(
			`Normalized ${assetKind} width ${stats.width_fraction.toFixed(3)} exceeds ${validation.max_subject_bounds_width_fraction}`,
		);
	}
	if (
		validation.min_subject_margin_fraction !== undefined &&
		stats.margin_fraction < validation.min_subject_margin_fraction
	) {
		throw new PipelineError(
			`Normalized ${assetKind} margin ${stats.margin_fraction.toFixed(3)} is below ${validation.min_subject_margin_fraction}`,
		);
	}
}

function normalizeCandidateFraming(candidate, config, assetKind) {
	const normalization = config.raw_output.framing_normalization;
	if (!normalization?.enabled) return candidate;
	const validation = validationForAsset(config, assetKind);
	const original = PNG.sync.read(candidate.buffer, { checkCRC: true });
	const before = framingStats(original);
	const height = framingHeightContract(validation, assetKind);
	const headroom = normalization.headroom_fraction;
	let scale = 1;
	if (height.maximum !== undefined) {
		scale = Math.min(
			scale,
			Math.max(0, height.maximum - headroom) / before.height_fraction,
		);
	}
	if (validation.max_subject_bounds_width_fraction !== undefined) {
		scale = Math.min(
			scale,
			Math.max(0, validation.max_subject_bounds_width_fraction - headroom) /
				before.width_fraction,
		);
	}
	const applied = scale < 1;
	const output = applied
		? resizeCandidateSubject(original, before.bounds, scale)
		: original;
	const after = framingStats(output);
	validateCandidateFraming(after, validation, assetKind);
	return {
		buffer: PNG.sync.write(output, { colorType: 6 }),
		stats: {
			...candidatePixelStats(output, config, assetKind),
			framing_normalization: {
				algorithm: normalization.algorithm,
				applied,
				scale: Math.min(1, scale),
				headroom_fraction: headroom,
				before,
				after,
			},
		},
	};
}

function splitPairedRaw(buffer, config) {
	let sheet;
	try {
		sheet = PNG.sync.read(buffer, { checkCRC: true });
	} catch (error) {
		throw new PipelineError(
			`Provider pair output is not a valid PNG: ${error.message}`,
		);
	}
	const expected = parseSize(config.provider.request.size);
	if (sheet.width !== expected.width || sheet.height !== expected.height) {
		throw new PipelineError(
			`Provider pair output is ${sheet.width}x${sheet.height}; expected ${expected.width}x${expected.height}`,
		);
	}
	const cell = parseSize(config.paired_output.cell_size);
	const children = {};
	for (const kind of ['portrait', 'marker']) {
		const origin = config.paired_output[kind];
		const png = new PNG({ width: cell.width, height: cell.height });
		for (let row = 0; row < cell.height; row += 1) {
			const sourceStart = ((origin.y + row) * sheet.width + origin.x) * 4;
			const targetStart = row * cell.width * 4;
			sheet.data.copy(
				png.data,
				targetStart,
				sourceStart,
				sourceStart + cell.width * 4,
			);
		}
		children[kind] = {
			buffer: PNG.sync.write(png, { colorType: 6 }),
		};
	}
	return {
		width: sheet.width,
		height: sheet.height,
		children,
	};
}

function validatePairedChildren(split, config) {
	const children = {};
	for (const kind of ['portrait', 'marker']) {
		try {
			const inspected = inspectRawPng(
				split.children[kind].buffer,
				config,
				kind,
				{
					allowOversizedFraming:
						config.raw_output.framing_normalization?.enabled === true,
				},
			);
			const candidate = normalizeCandidateFraming(
				chromaKeyCandidate(inspected.png, config, kind),
				config,
				kind,
			);
			children[kind] = {
				raw: split.children[kind].buffer,
				inspected,
				candidate,
			};
		} catch (error) {
			throw new PipelineError(
				`${kind[0].toUpperCase()}${kind.slice(1)} cell failed validation: ${error.message}`,
			);
		}
	}
	return children;
}

function retryAfterMilliseconds(response) {
	const raw = response.headers.get('retry-after');
	if (!raw) return null;
	const seconds = Number(raw);
	if (Number.isFinite(seconds)) return Math.max(0, seconds * 1000);
	const date = Date.parse(raw);
	return Number.isNaN(date) ? null : Math.max(0, date - Date.now());
}

function effectiveBaseUrl(config, environment) {
	const override = config.provider.base_url_env
		? environment[config.provider.base_url_env]
		: null;
	const value = String(override || config.provider.base_url).replace(
		/\/+$/,
		'',
	);
	let parsed;
	try {
		parsed = new URL(value);
	} catch {
		throw new PipelineError(`Provider base URL is invalid: ${value}`);
	}
	if (parsed.username || parsed.password) {
		throw new PipelineError('Provider base URL must not contain credentials');
	}
	const loopback =
		parsed.hostname === '127.0.0.1' || parsed.hostname === 'localhost';
	if (
		parsed.protocol !== 'https:' &&
		!(loopback && parsed.protocol === 'http:')
	) {
		throw new PipelineError(
			'Provider base URL must use HTTPS (HTTP is allowed only for loopback tests)',
		);
	}
	return value;
}

async function callOpenAiImagesEdit({
	config,
	references,
	prompt,
	environment,
	fetchImpl,
}) {
	const apiKey = environment[config.provider.api_key_env];
	if (!apiKey?.trim()) {
		throw new PipelineError(
			`${config.provider.api_key_env} is not set; live generation cannot start`,
		);
	}
	const form = new FormData();
	form.append('model', config.provider.model);
	form.append('prompt', prompt);
	form.append('n', '1');
	for (const [key, value] of Object.entries(config.provider.request)) {
		form.append(key, String(value));
	}
	for (const reference of references) {
		form.append(
			'image[]',
			new Blob([reference.buffer], { type: 'image/png' }),
			basename(reference.path),
		);
	}

	const headers = {
		Authorization: `Bearer ${apiKey.trim()}`,
	};
	const organization = config.provider.organization_env
		? environment[config.provider.organization_env]
		: null;
	const project = config.provider.project_env
		? environment[config.provider.project_env]
		: null;
	if (organization?.trim())
		headers['OpenAI-Organization'] = organization.trim();
	if (project?.trim()) headers['OpenAI-Project'] = project.trim();
	const url = `${effectiveBaseUrl(config, environment)}${config.provider.endpoint}`;
	let response;
	try {
		response = await fetchImpl(url, {
			method: 'POST',
			headers,
			body: form,
			signal: AbortSignal.timeout(config.retry.request_timeout_ms),
		});
	} catch (error) {
		throw new ProviderError(`OpenAI image request failed: ${error.message}`);
	}
	if (!response.ok) {
		const body = (await response.text()).slice(0, 4000);
		throw new ProviderError(
			`OpenAI image request returned HTTP ${response.status}: ${body}`,
			{
				status: response.status,
				retryAfterMs: retryAfterMilliseconds(response),
			},
		);
	}
	let body;
	try {
		body = await response.json();
	} catch (error) {
		throw new ProviderError(
			`OpenAI image response was not JSON: ${error.message}`,
			{
				status: response.status,
			},
		);
	}
	const encoded = body.data?.[0]?.b64_json;
	if (typeof encoded !== 'string' || encoded.length === 0) {
		throw new ProviderError(
			'OpenAI image response did not contain data[0].b64_json',
			{
				status: response.status,
			},
		);
	}
	const buffer = Buffer.from(encoded, 'base64');
	if (buffer.length > 64 * 1024 * 1024) {
		throw new ProviderError(
			'OpenAI image response exceeded the 64 MiB candidate limit',
			{
				status: response.status,
			},
		);
	}
	return {
		buffer,
		requestId: response.headers.get('x-request-id'),
		providerCreatedAt: body.created ?? null,
		usage: body.usage ?? null,
	};
}

function createRateGate(requestsPerMinute) {
	const spacing = Math.ceil(60_000 / requestsPerMinute);
	let tail = Promise.resolve();
	let nextStart = 0;
	return async () => {
		let release;
		const current = new Promise((resolveCurrent) => {
			release = resolveCurrent;
		});
		const previous = tail;
		tail = current;
		await previous;
		try {
			const wait = Math.max(0, nextStart - Date.now());
			if (wait > 0) await sleep(wait);
			nextStart = Date.now() + spacing;
		} finally {
			release();
		}
	};
}

async function generateWithRetry(context, rateGate) {
	const { config } = context;
	let lastError;
	for (let attempt = 1; attempt <= config.retry.max_attempts; attempt += 1) {
		await rateGate();
		try {
			return {
				...(await callOpenAiImagesEdit(context)),
				attempts: attempt,
			};
		} catch (error) {
			lastError = error;
			const retryable = error instanceof ProviderError && error.retryable;
			if (!retryable || attempt === config.retry.max_attempts) break;
			const exponential = Math.min(
				config.retry.max_delay_ms,
				config.retry.initial_delay_ms * 2 ** (attempt - 1),
			);
			await sleep(Math.max(exponential, error.retryAfterMs ?? 0));
		}
	}
	throw lastError;
}

function safeError(error) {
	return {
		name: error.name,
		message: String(error.message).slice(0, 4000),
		status: error instanceof ProviderError ? error.status : null,
	};
}

async function assertProviderInputsMatchForReprocessing(
	pipeline,
	job,
	failure,
) {
	const mismatches = [];
	if (failure.provenance?.prompt_sha256 !== job.promptSha256) {
		mismatches.push('prompt');
	}
	const expectedReferences = job.references.map(({ id, sha256: hash }) => ({
		id,
		sha256: hash,
	}));
	const failureReferences = (failure.provenance?.reference_inputs ?? []).map(
		({ id, sha256: hash }) => ({ id, sha256: hash }),
	);
	if (canonicalJson(failureReferences) !== canonicalJson(expectedReferences)) {
		mismatches.push('references');
	}
	const currentProvider = {
		id: pipeline.config.provider.id,
		adapter: pipeline.config.provider.adapter,
		model: pipeline.config.provider.model,
		endpoint: pipeline.config.provider.endpoint,
		request: pipeline.config.provider.request,
	};
	const failureProvider = {
		id: failure.provider?.id,
		adapter: failure.provider?.adapter,
		model: failure.provider?.model,
		endpoint: failure.provider?.endpoint,
		request: failure.provider?.request,
	};
	if (canonicalJson(failureProvider) !== canonicalJson(currentProvider)) {
		mismatches.push('provider request');
	}
	try {
		const inputRecordPath = resolvePath(
			pipeline.repoRoot,
			failure.provenance?.input_record_path ?? '',
		);
		const previousRecord = JSON.parse(await readFile(inputRecordPath, 'utf8'));
		if (sha256(canonicalJson(previousRecord)) !== job.inputRecordSha256) {
			mismatches.push('NPC input record');
		}
	} catch {
		mismatches.push('NPC input record');
	}
	if (mismatches.length > 0) {
		throw new PipelineError(
			`Failure receipt cannot be reprocessed under the current provider inputs; changed: ${mismatches.join(', ')}`,
		);
	}
}

function providerReceipt(pipeline, environment, generated = null) {
	return {
		id: pipeline.config.provider.id,
		adapter: pipeline.config.provider.adapter,
		base_url: effectiveBaseUrl(pipeline.config, environment),
		model: pipeline.config.provider.model,
		endpoint: pipeline.config.provider.endpoint,
		request: pipeline.config.provider.request,
		request_id: generated?.requestId ?? null,
		provider_created_at: generated?.providerCreatedAt ?? null,
		attempts: generated?.attempts ?? null,
		usage: generated?.usage ?? null,
	};
}

function provenanceReceipt(pipeline, job, paths) {
	return {
		config_path: receiptPath(pipeline.repoRoot, pipeline.configPath),
		config_sha256: pipeline.configSha256,
		inputs_path: receiptPath(pipeline.repoRoot, pipeline.inputsPath),
		inputs_sha256: pipeline.inputsSha256,
		prompt_path: receiptPath(pipeline.repoRoot, paths.prompt),
		prompt_sha256: job.promptSha256,
		input_record_path: receiptPath(pipeline.repoRoot, paths.inputRecord),
		reference_inputs: job.references.map((reference) => ({
			id: reference.id,
			path: receiptPath(pipeline.repoRoot, reference.path),
			purpose: reference.purpose,
			sha256: reference.sha256,
		})),
	};
}

function candidateReceipt({
	pipeline,
	job,
	paths,
	environment,
	generated,
	rawPath,
	candidatePath,
	inspected,
	candidate,
	startedAt,
	provider = null,
	reprocessing = null,
}) {
	return {
		schema_version: 1,
		receipt_type: 'notebook-person-art-candidate',
		job_id: job.jobId,
		status: pipeline.config.approval.generated_status,
		review: {
			status: pipeline.config.approval.review_status,
			reviewer: null,
			reviewed_at: null,
			notes: null,
		},
		promotion: {
			eligible: false,
			reason: 'Human review and explicit approval are required',
		},
		subject: {
			kind: job.subject.subjectKind,
			npc_id: job.subject.npcId,
			name: job.subject.name,
			input_record_sha256: job.inputRecordSha256,
		},
		asset: {
			kind: job.assetKind,
			candidate_index: job.candidateIndex,
		},
		provider: provider ?? providerReceipt(pipeline, environment, generated),
		provenance: provenanceReceipt(pipeline, job, paths),
		artifact: {
			raw_path: receiptPath(pipeline.repoRoot, rawPath),
			raw_sha256: sha256(generated.buffer),
			candidate_path: receiptPath(pipeline.repoRoot, candidatePath),
			candidate_sha256: sha256(candidate.buffer),
			media_type: 'image/png',
			width: inspected.stats.width,
			height: inspected.stats.height,
			key_color: pipeline.config.raw_output.key_color,
			raw_validation: inspected.stats,
			candidate_validation: candidate.stats,
		},
		reprocessing,
		timing: {
			started_at: startedAt,
			completed_at: new Date().toISOString(),
		},
	};
}

function pairedCandidateReceipt({
	pipeline,
	job,
	paths,
	environment,
	generated,
	rawPath,
	split,
	children,
	childPaths,
	startedAt,
	provider = null,
	reprocessing = null,
}) {
	const childArtifacts = {};
	for (const kind of ['portrait', 'marker']) {
		childArtifacts[kind] = {
			raw_path: receiptPath(pipeline.repoRoot, childPaths[kind].raw),
			raw_sha256: sha256(children[kind].raw),
			candidate_path: receiptPath(
				pipeline.repoRoot,
				childPaths[kind].candidate,
			),
			candidate_sha256: sha256(children[kind].candidate.buffer),
			media_type: 'image/png',
			width: children[kind].inspected.stats.width,
			height: children[kind].inspected.stats.height,
			raw_validation: children[kind].inspected.stats,
			candidate_validation: children[kind].candidate.stats,
		};
	}
	return {
		schema_version: 1,
		receipt_type: 'notebook-person-art-pair-candidate',
		job_id: job.jobId,
		status: pipeline.config.approval.generated_status,
		review: {
			status: pipeline.config.approval.review_status,
			reviewer: null,
			reviewed_at: null,
			notes: null,
		},
		promotion: {
			eligible: false,
			reason:
				'Atomic human approval of both assets and their shared identity is required',
		},
		identity_lock: {
			generation: 'single-provider-request',
			status: 'pending-human-review',
			rerender_policy:
				'If either child fails, rerun and review the portrait-marker pair together',
		},
		subject: {
			kind: job.subject.subjectKind,
			npc_id: job.subject.npcId,
			name: job.subject.name,
			input_record_sha256: job.inputRecordSha256,
		},
		asset: {
			kind: 'pair',
			candidate_index: job.candidateIndex,
			children: ['portrait', 'marker'],
		},
		provider: provider ?? providerReceipt(pipeline, environment, generated),
		provenance: provenanceReceipt(pipeline, job, paths),
		artifact: {
			raw_path: receiptPath(pipeline.repoRoot, rawPath),
			raw_sha256: sha256(generated.buffer),
			media_type: 'image/png',
			width: split.width,
			height: split.height,
			key_color: pipeline.config.raw_output.key_color,
			layout: pipeline.config.paired_output,
			children: childArtifacts,
		},
		reprocessing,
		timing: {
			started_at: startedAt,
			completed_at: new Date().toISOString(),
		},
	};
}

async function executeJob({
	pipeline,
	job,
	paths,
	runId,
	environment,
	fetchImpl,
	rateGate,
}) {
	await mkdir(paths.root, { recursive: true });
	let lock;
	try {
		lock = await open(paths.lock, 'wx');
	} catch (error) {
		if (error.code === 'EEXIST') {
			throw new PipelineError(
				`Job ${job.jobId} is already locked by another runner`,
			);
		}
		throw error;
	}
	const startedAt = new Date().toISOString();
	const attemptId = `${runId}-${startedAt.replaceAll(/[^0-9]/g, '')}-${randomUUID()}`;
	const attemptRoot = join(paths.root, 'attempts', attemptId);
	const rawPath = join(attemptRoot, 'raw.png');
	const candidatePath = join(attemptRoot, 'candidate.png');
	const childPaths = Object.fromEntries(
		['portrait', 'marker'].map((kind) => [
			kind,
			{
				raw: join(attemptRoot, `${kind}-raw.png`),
				candidate: join(attemptRoot, `${kind}-candidate.png`),
			},
		]),
	);
	const failurePath = join(attemptRoot, 'failure.json');
	let generated = null;
	let rawPersisted = false;
	let split = null;
	let childRawsPersisted = false;
	try {
		await Promise.all([
			atomicWrite(paths.prompt, `${job.prompt}\n`),
			atomicWrite(paths.inputRecord, prettyJson(job.subject.record)),
		]);
		generated = await generateWithRetry(
			{
				config: pipeline.config,
				references: job.references,
				prompt: job.prompt,
				environment,
				fetchImpl,
			},
			rateGate,
		);
		await atomicWrite(rawPath, generated.buffer);
		rawPersisted = true;
		let receipt;
		if (job.assetKind === 'pair') {
			split = splitPairedRaw(generated.buffer, pipeline.config);
			await Promise.all(
				['portrait', 'marker'].map((kind) =>
					atomicWrite(childPaths[kind].raw, split.children[kind].buffer),
				),
			);
			childRawsPersisted = true;
			const children = validatePairedChildren(split, pipeline.config);
			await Promise.all(
				['portrait', 'marker'].map((kind) =>
					atomicWrite(
						childPaths[kind].candidate,
						children[kind].candidate.buffer,
					),
				),
			);
			receipt = pairedCandidateReceipt({
				pipeline,
				job,
				paths,
				environment,
				generated,
				rawPath,
				split,
				children,
				childPaths,
				startedAt,
			});
		} else {
			const inspected = inspectRawPng(
				generated.buffer,
				pipeline.config,
				job.assetKind,
				{
					allowOversizedFraming:
						pipeline.config.raw_output.framing_normalization?.enabled === true,
				},
			);
			const candidate = normalizeCandidateFraming(
				chromaKeyCandidate(inspected.png, pipeline.config, job.assetKind),
				pipeline.config,
				job.assetKind,
			);
			await atomicWrite(candidatePath, candidate.buffer);
			receipt = candidateReceipt({
				pipeline,
				job,
				paths,
				environment,
				generated,
				rawPath,
				candidatePath,
				inspected,
				candidate,
				startedAt,
			});
		}
		receipt.run_id = runId;
		await atomicWrite(paths.receipt, prettyJson(receipt));
		return {
			job_id: job.jobId,
			status: 'generated',
			receipt: receiptPath(pipeline.repoRoot, paths.receipt),
		};
	} catch (error) {
		const failure = {
			schema_version: 1,
			receipt_type: 'notebook-person-art-generation-failure',
			job_id: job.jobId,
			run_id: runId,
			status: 'failed',
			subject: {
				kind: job.subject.subjectKind,
				npc_id: job.subject.npcId,
				name: job.subject.name,
			},
			asset_kind: job.assetKind,
			candidate_index: job.candidateIndex,
			provider: providerReceipt(pipeline, environment, generated),
			provenance: provenanceReceipt(pipeline, job, paths),
			artifact: generated
				? {
						raw_path: rawPersisted
							? receiptPath(pipeline.repoRoot, rawPath)
							: null,
						raw_sha256: sha256(generated.buffer),
						raw_persisted: rawPersisted,
						media_type: 'image/png',
						children:
							job.assetKind === 'pair' && split
								? Object.fromEntries(
										['portrait', 'marker'].map((kind) => [
											kind,
											{
												raw_path: childRawsPersisted
													? receiptPath(pipeline.repoRoot, childPaths[kind].raw)
													: null,
												raw_sha256: sha256(split.children[kind].buffer),
												raw_persisted: childRawsPersisted,
											},
										]),
									)
								: null,
					}
				: null,
			error: safeError(error),
			started_at: startedAt,
			failed_at: new Date().toISOString(),
		};
		await atomicWrite(failurePath, prettyJson(failure));
		return {
			job_id: job.jobId,
			status: 'failed',
			failure: receiptPath(pipeline.repoRoot, failurePath),
			error: failure.error,
		};
	} finally {
		await lock.close();
		await rm(paths.lock, { force: true });
	}
}

function defaultRunId(pipeline, selection) {
	const fingerprint = sha256(
		canonicalJson({
			pipeline_revision: pipeline.config.pipeline_revision,
			config_sha256: pipeline.configSha256,
			inputs_sha256: pipeline.inputsSha256,
			job_ids: selection.jobs.map((job) => job.jobId),
			shard_index: selection.shardIndex,
			shard_count: selection.shardCount,
		}),
	);
	return `run-${fingerprint.slice(0, 16)}`;
}

function validateRunId(runId) {
	if (!/^[a-zA-Z0-9._-]+$/.test(runId)) {
		throw new PipelineError(
			`--run-id may contain only letters, numbers, dot, underscore, and hyphen`,
		);
	}
}

async function mapConcurrent(items, concurrency, worker) {
	const results = new Array(items.length);
	let nextIndex = 0;
	async function runWorker() {
		while (true) {
			const index = nextIndex;
			nextIndex += 1;
			if (index >= items.length) return;
			results[index] = await worker(items[index], index);
		}
	}
	await Promise.all(
		Array.from({ length: Math.min(concurrency, items.length) }, () =>
			runWorker(),
		),
	);
	return results;
}

async function reprocessCandidateSource(options, sourceKind) {
	const sourceOption =
		sourceKind === 'failure' ? options.failurePath : options.receiptPath;
	if (!sourceOption) {
		throw new PipelineError(
			sourceKind === 'failure'
				? '--reprocess-failure requires a failure receipt path'
				: '--reprocess-receipt requires a candidate receipt path',
		);
	}
	const pipeline = await loadPipeline(options);
	const sourcePath = resolvePath(pipeline.repoRoot, sourceOption);
	const source = (
		await loadJsonWithHash(
			sourcePath,
			sourceKind === 'failure' ? 'failure receipt' : 'candidate receipt',
		)
	).value;
	const isFailure =
		source.receipt_type === 'notebook-person-art-generation-failure' &&
		source.status === 'failed';
	const isCandidate =
		(source.receipt_type === 'notebook-person-art-pair-candidate' ||
			source.receipt_type === 'notebook-person-art-candidate') &&
		source.status === 'candidate';
	if (
		(sourceKind === 'failure' && !isFailure) ||
		(sourceKind === 'receipt' && !isCandidate)
	) {
		throw new PipelineError(
			`${sourcePath} is not a ${sourceKind === 'failure' ? 'generation failure' : 'candidate'} receipt`,
		);
	}
	if (
		!source.artifact?.raw_path ||
		(sourceKind === 'failure' && !source.artifact.raw_persisted)
	) {
		throw new PipelineError(
			`${sourceKind === 'failure' ? 'Failure' : 'Candidate'} receipt has no preserved raw provider artifact`,
		);
	}
	const rawPath = resolvePath(pipeline.repoRoot, source.artifact.raw_path);
	const raw = await readFile(rawPath);
	if (sha256(raw) !== source.artifact.raw_sha256) {
		throw new PipelineError(
			'Preserved raw artifact hash does not match source receipt',
		);
	}

	const candidateIndex = parsePositiveInteger(
		source.candidate_index ?? source.asset?.candidate_index,
		'source candidate_index',
	);
	const assetKind = source.asset_kind ?? source.asset?.kind;
	const selection = buildJobs(pipeline, {
		npcIds: source.subject?.kind === 'npc' ? [source.subject.npc_id] : [],
		includeFallback: source.subject?.kind === 'fallback',
		asset: assetKind,
		candidateCount: candidateIndex,
	});
	const job = selection.jobs.find(
		(candidateJob) => candidateJob.candidateIndex === candidateIndex,
	);
	if (!job) {
		throw new PipelineError(
			'Source receipt does not match a current candidate job',
		);
	}
	if (job.jobId !== source.job_id) {
		await assertProviderInputsMatchForReprocessing(pipeline, job, source);
	}
	const configuredRoot = options.outputRoot ?? pipeline.config.storage.root;
	const outputRoot = resolvePath(pipeline.repoRoot, configuredRoot);
	const paths = objectPaths(outputRoot, job);
	const existing = await existingReceipt(pipeline.repoRoot, paths, job);
	if (existing) {
		return {
			status: 'resume-skip',
			receiptPath: existing.receiptPath,
			receipt: existing.receipt,
		};
	}

	await mkdir(paths.root, { recursive: true });
	let lock;
	try {
		lock = await open(paths.lock, 'wx');
	} catch (error) {
		if (error.code === 'EEXIST') {
			throw new PipelineError(`Job ${job.jobId} is locked by another runner`);
		}
		throw error;
	}
	try {
		const reprocessId = `reprocess-${new Date().toISOString().replaceAll(/[^0-9]/g, '')}-${randomUUID()}`;
		const attemptRoot = join(paths.root, 'attempts', reprocessId);
		await Promise.all([
			atomicWrite(paths.prompt, `${job.prompt}\n`),
			atomicWrite(paths.inputRecord, prettyJson(job.subject.record)),
		]);
		const generated = {
			buffer: raw,
			requestId: source.provider?.request_id ?? null,
			providerCreatedAt: source.provider?.provider_created_at ?? null,
			attempts: source.provider?.attempts ?? null,
			usage: source.provider?.usage ?? null,
		};
		const startedAt = new Date().toISOString();
		const reprocessing = {
			...(sourceKind === 'failure'
				? {
						source_failure_path: receiptPath(pipeline.repoRoot, sourcePath),
						source_failure_error: source.error,
					}
				: {
						source_receipt_path: receiptPath(pipeline.repoRoot, sourcePath),
						source_candidate_sha256:
							source.artifact?.children?.portrait?.candidate_sha256 ??
							source.artifact?.candidate_sha256 ??
							null,
					}),
			source_job_id: source.job_id,
			provider_request_reused: true,
			reprocessed_at: startedAt,
		};
		let receipt;
		if (job.assetKind === 'pair') {
			const split = splitPairedRaw(raw, pipeline.config);
			const childPaths = Object.fromEntries(
				['portrait', 'marker'].map((kind) => [
					kind,
					{
						raw: join(attemptRoot, `${kind}-raw.png`),
						candidate: join(attemptRoot, `${kind}-candidate.png`),
					},
				]),
			);
			await Promise.all(
				['portrait', 'marker'].map((kind) =>
					atomicWrite(childPaths[kind].raw, split.children[kind].buffer),
				),
			);
			const children = validatePairedChildren(split, pipeline.config);
			await Promise.all(
				['portrait', 'marker'].map((kind) =>
					atomicWrite(
						childPaths[kind].candidate,
						children[kind].candidate.buffer,
					),
				),
			);
			receipt = pairedCandidateReceipt({
				pipeline,
				job,
				paths,
				environment: options.environment ?? process.env,
				generated,
				rawPath,
				split,
				children,
				childPaths,
				startedAt,
				provider: source.provider,
				reprocessing,
			});
		} else {
			const inspected = inspectRawPng(raw, pipeline.config, job.assetKind, {
				allowOversizedFraming:
					pipeline.config.raw_output.framing_normalization?.enabled === true,
			});
			const candidate = normalizeCandidateFraming(
				chromaKeyCandidate(inspected.png, pipeline.config, job.assetKind),
				pipeline.config,
				job.assetKind,
			);
			const candidatePath = join(attemptRoot, 'candidate.png');
			await atomicWrite(candidatePath, candidate.buffer);
			receipt = candidateReceipt({
				pipeline,
				job,
				paths,
				environment: options.environment ?? process.env,
				generated,
				rawPath,
				candidatePath,
				inspected,
				candidate,
				startedAt,
				provider: source.provider,
				reprocessing,
			});
		}
		receipt.run_id = options.runId ?? source.run_id;
		await atomicWrite(paths.receipt, prettyJson(receipt));
		return {
			status: 'reprocessed',
			receiptPath: receiptPath(pipeline.repoRoot, paths.receipt),
			receipt,
		};
	} finally {
		await lock.close();
		await rm(paths.lock, { force: true });
	}
}

export async function reprocessCandidateFailure(options = {}) {
	return reprocessCandidateSource(options, 'failure');
}

export async function reprocessCandidateReceipt(options = {}) {
	return reprocessCandidateSource(options, 'receipt');
}

export async function reprocessCandidateManifest(options = {}) {
	if (!options.manifestPath) {
		throw new PipelineError(
			'--reprocess-manifest requires a generation manifest path',
		);
	}
	const pipeline = await loadPipeline(options);
	const sourceManifestPath = resolvePath(
		pipeline.repoRoot,
		options.manifestPath,
	);
	const sourceEntries = (await readFile(sourceManifestPath, 'utf8'))
		.split('\n')
		.map((line) => line.trim())
		.filter(Boolean)
		.map((line, index) => {
			try {
				return JSON.parse(line);
			} catch (error) {
				throw new PipelineError(
					`Reprocess manifest line ${index + 1} is invalid JSON: ${error.message}`,
				);
			}
		});
	if (sourceEntries.length === 0) {
		throw new PipelineError('Reprocess manifest contains no candidate entries');
	}
	for (const entry of sourceEntries) {
		if (!entry.receipt && !entry.failure) {
			throw new PipelineError(
				`Reprocess manifest entry ${entry.job_id ?? 'unknown'} has neither receipt nor failure`,
			);
		}
	}
	const runId =
		options.runId ??
		`reprocess-${sha256(await readFile(sourceManifestPath)).slice(0, 16)}`;
	validateRunId(runId);
	const configuredRoot = options.outputRoot ?? pipeline.config.storage.root;
	const outputRoot = resolvePath(pipeline.repoRoot, configuredRoot);
	const runRoot = join(outputRoot, 'runs', runId);
	const manifestPath = join(runRoot, 'manifest.jsonl');
	const startedAt = new Date().toISOString();
	await mkdir(runRoot, { recursive: true });
	await atomicWrite(
		join(runRoot, 'run.json'),
		prettyJson({
			schema_version: 1,
			run_id: runId,
			mode: 'local-reprocess',
			status: 'running',
			source_manifest: receiptPath(pipeline.repoRoot, sourceManifestPath),
			source_entries: sourceEntries.length,
			provider_requests: 0,
			pipeline_revision: pipeline.config.pipeline_revision,
			config_sha256: pipeline.configSha256,
			started_at: startedAt,
		}),
	);
	const concurrency = Math.min(
		parsePositiveInteger(
			options.concurrency ?? pipeline.config.rate_limit.max_concurrency,
			'--concurrency',
		),
		pipeline.config.rate_limit.max_concurrency,
	);
	const results = await mapConcurrent(
		sourceEntries,
		concurrency,
		async (entry) => {
			const sourcePath = entry.receipt ?? entry.failure;
			try {
				const result = entry.failure
					? await reprocessCandidateFailure({
							...options,
							failurePath: sourcePath,
							runId,
						})
					: await reprocessCandidateReceipt({
							...options,
							receiptPath: sourcePath,
							runId,
						});
				return {
					source_job_id: entry.job_id,
					source: sourcePath,
					status: result.status,
					job_id: result.receipt.job_id,
					receipt: result.receiptPath,
				};
			} catch (error) {
				return {
					source_job_id: entry.job_id,
					source: sourcePath,
					status: 'failed',
					error: safeError(error),
				};
			}
		},
	);
	await atomicWrite(
		manifestPath,
		`${results.map((result) => JSON.stringify(result)).join('\n')}\n`,
	);
	const failed = results.filter((result) => result.status === 'failed').length;
	const summary = {
		schema_version: 1,
		run_id: runId,
		mode: 'local-reprocess',
		status: failed === 0 ? 'completed' : 'failed',
		source_manifest: receiptPath(pipeline.repoRoot, sourceManifestPath),
		source_entries: sourceEntries.length,
		provider_requests: 0,
		pipeline_revision: pipeline.config.pipeline_revision,
		config_sha256: pipeline.configSha256,
		started_at: startedAt,
		completed_at: new Date().toISOString(),
		result_counts: {
			reprocessed: results.filter((result) => result.status === 'reprocessed')
				.length,
			resume_skip: results.filter((result) => result.status === 'resume-skip')
				.length,
			failed,
		},
	};
	await atomicWrite(join(runRoot, 'run.json'), prettyJson(summary));
	if (failed > 0) {
		throw new PipelineError(
			`${failed} local reprocessing job(s) failed; see ${receiptPath(pipeline.repoRoot, manifestPath)}`,
		);
	}
	return { plan: summary, results };
}

export async function runCandidateGeneration(options = {}) {
	const pipeline = await loadPipeline(options);
	const selection = buildJobs(pipeline, options);
	const configuredRoot = options.outputRoot ?? pipeline.config.storage.root;
	const outputRoot = resolvePath(pipeline.repoRoot, configuredRoot);
	const runId = options.runId ?? defaultRunId(pipeline, selection);
	validateRunId(runId);
	const existing = new Map();
	for (const job of selection.jobs) {
		const paths = objectPaths(outputRoot, job);
		const receipt = await existingReceipt(pipeline.repoRoot, paths, job);
		if (receipt) existing.set(job.jobId, receipt);
	}
	const pending = selection.jobs.filter((job) => !existing.has(job.jobId));
	const plan = {
		schema_version: 1,
		run_id: runId,
		mode: options.execute ? 'execute' : 'plan',
		pipeline_revision: pipeline.config.pipeline_revision,
		provider: {
			id: pipeline.config.provider.id,
			adapter: pipeline.config.provider.adapter,
			model: pipeline.config.provider.model,
			request: pipeline.config.provider.request,
		},
		config_path: receiptPath(pipeline.repoRoot, pipeline.configPath),
		config_sha256: pipeline.configSha256,
		inputs_path: receiptPath(pipeline.repoRoot, pipeline.inputsPath),
		inputs_sha256: pipeline.inputsSha256,
		output_root: receiptPath(pipeline.repoRoot, outputRoot),
		shard: {
			index: selection.shardIndex,
			count: selection.shardCount,
			unsharded_jobs: selection.unshardedJobCount,
			selected_jobs: selection.jobs.length,
		},
		requests: {
			planned: selection.jobs.length,
			resumable_existing: existing.size,
			pending: pending.length,
		},
		approval: pipeline.config.approval,
		jobs: selection.jobs.map((job) => ({
			job_id: job.jobId,
			subject_kind: job.subject.subjectKind,
			npc_id: job.subject.npcId,
			name: job.subject.name,
			asset_kind: job.assetKind,
			candidate_index: job.candidateIndex,
			status: existing.has(job.jobId) ? 'resume-skip' : 'pending',
		})),
	};
	if (!options.execute) return { plan, results: [] };

	const maxRequests = parseNonNegativeInteger(
		options.maxRequests,
		'--max-requests',
	);
	if (pending.length > maxRequests) {
		throw new PipelineError(
			`Generation would make ${pending.length} provider requests, exceeding --max-requests ${maxRequests}`,
		);
	}
	const environment = options.environment ?? process.env;
	if (
		pending.length > 0 &&
		!environment[pipeline.config.provider.api_key_env]?.trim()
	) {
		throw new PipelineError(
			`${pipeline.config.provider.api_key_env} is not set; run plan mode now and expose the key only for --execute`,
		);
	}
	const concurrency = Math.min(
		parsePositiveInteger(
			options.concurrency ?? pipeline.config.rate_limit.max_concurrency,
			'--concurrency',
		),
		pipeline.config.rate_limit.max_concurrency,
	);
	const runRoot = join(outputRoot, 'runs', runId);
	const manifestPath = join(runRoot, 'manifest.jsonl');
	await mkdir(runRoot, { recursive: true });
	await atomicWrite(
		join(runRoot, 'run.json'),
		prettyJson({
			...plan,
			status: 'running',
			started_at: new Date().toISOString(),
		}),
	);
	await atomicWrite(manifestPath, '');
	let appendQueue = Promise.resolve();
	const appendResult = (result) => {
		appendQueue = appendQueue.then(() =>
			appendFile(manifestPath, `${JSON.stringify(result)}\n`),
		);
		return appendQueue;
	};
	const initialResults = [];
	for (const job of selection.jobs) {
		const found = existing.get(job.jobId);
		if (found) {
			const result = {
				job_id: job.jobId,
				status: 'resume-skip',
				receipt: found.receiptPath,
			};
			initialResults.push(result);
			await appendResult(result);
		}
	}
	const rateGate = createRateGate(
		pipeline.config.rate_limit.requests_per_minute,
	);
	const generatedResults = await mapConcurrent(
		pending,
		concurrency,
		async (job) => {
			const result = await executeJob({
				pipeline,
				job,
				paths: objectPaths(outputRoot, job),
				runId,
				environment,
				fetchImpl: options.fetchImpl ?? fetch,
				rateGate,
			});
			await appendResult(result);
			return result;
		},
	);
	await appendQueue;
	const results = [...initialResults, ...generatedResults];
	const failed = results.filter((result) => result.status === 'failed').length;
	const summary = {
		...plan,
		status: failed === 0 ? 'completed' : 'failed',
		completed_at: new Date().toISOString(),
		result_counts: {
			generated: results.filter((result) => result.status === 'generated')
				.length,
			resume_skip: results.filter((result) => result.status === 'resume-skip')
				.length,
			failed,
		},
	};
	await atomicWrite(join(runRoot, 'run.json'), prettyJson(summary));
	if (failed > 0) {
		throw new PipelineError(
			`${failed} candidate generation job(s) failed; see ${receiptPath(pipeline.repoRoot, manifestPath)}`,
		);
	}
	return { plan: summary, results };
}

function cliOptions() {
	const parsed = parseArgs({
		options: {
			config: { type: 'string' },
			inputs: { type: 'string' },
			'env-file': { type: 'string' },
			'output-root': { type: 'string' },
			execute: { type: 'boolean', default: false },
			'max-requests': { type: 'string' },
			'npc-id': { type: 'string', multiple: true },
			asset: { type: 'string', default: 'all' },
			candidates: { type: 'string' },
			'include-fallback': { type: 'boolean' },
			'exclude-fallback': { type: 'boolean', default: false },
			'shard-index': { type: 'string', default: '0' },
			'shard-count': { type: 'string', default: '1' },
			concurrency: { type: 'string' },
			'run-id': { type: 'string' },
			'reprocess-failure': { type: 'string' },
			'reprocess-receipt': { type: 'string' },
			'reprocess-manifest': { type: 'string' },
		},
		strict: true,
	});
	if (parsed.values['include-fallback'] && parsed.values['exclude-fallback']) {
		throw new PipelineError(
			'Use only one of --include-fallback or --exclude-fallback',
		);
	}
	if (parsed.values.execute && parsed.values['max-requests'] === undefined) {
		throw new PipelineError(
			'--execute requires an explicit --max-requests cap',
		);
	}
	const reprocessModes = [
		parsed.values['reprocess-failure'],
		parsed.values['reprocess-receipt'],
		parsed.values['reprocess-manifest'],
	].filter(Boolean);
	if (reprocessModes.length > 1) {
		throw new PipelineError(
			'Use only one of --reprocess-failure, --reprocess-receipt, or --reprocess-manifest',
		);
	}
	if (reprocessModes.length > 0 && parsed.values.execute) {
		throw new PipelineError(
			'Reprocessing is local-only and cannot be combined with --execute',
		);
	}
	let includeFallback;
	if (parsed.values['include-fallback']) includeFallback = true;
	if (parsed.values['exclude-fallback']) includeFallback = false;
	return {
		configPath: parsed.values.config,
		inputsPath: parsed.values.inputs,
		envFile: parsed.values['env-file'],
		outputRoot: parsed.values['output-root'],
		execute: parsed.values.execute,
		maxRequests: parsed.values['max-requests'],
		npcIds: parsed.values['npc-id'],
		asset: parsed.values.asset,
		candidateCount: parsed.values.candidates,
		includeFallback,
		shardIndex: parsed.values['shard-index'],
		shardCount: parsed.values['shard-count'],
		concurrency: parsed.values.concurrency,
		runId: parsed.values['run-id'],
		failurePath: parsed.values['reprocess-failure'],
		receiptPath: parsed.values['reprocess-receipt'],
		manifestPath: parsed.values['reprocess-manifest'],
	};
}

async function main() {
	const options = cliOptions();
	if (options.envFile) {
		try {
			process.loadEnvFile(resolve(options.envFile));
		} catch (error) {
			throw new PipelineError(
				`Could not load --env-file ${options.envFile}: ${error.message}`,
			);
		}
	}
	if (options.manifestPath) {
		const result = await reprocessCandidateManifest(options);
		console.log(prettyJson(result.plan).trimEnd());
		return;
	}
	if (options.failurePath || options.receiptPath) {
		const result = options.failurePath
			? await reprocessCandidateFailure(options)
			: await reprocessCandidateReceipt(options);
		console.log(
			prettyJson({
				status: result.status,
				receipt: result.receiptPath,
				job_id: result.receipt.job_id,
			}).trimEnd(),
		);
		return;
	}
	const result = await runCandidateGeneration(options);
	console.log(prettyJson(result.plan).trimEnd());
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
