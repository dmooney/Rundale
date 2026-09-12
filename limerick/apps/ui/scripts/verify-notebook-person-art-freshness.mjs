import { createHash } from 'node:crypto';
import { readFile, stat } from 'node:fs/promises';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const here = dirname(fileURLToPath(import.meta.url));
const defaultUiRoot = resolve(here, '..');
const defaultRepoRoot = resolve(defaultUiRoot, '../../..');

function sha256(bytes) {
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

function portablePath(path) {
	return path.split(sep).join('/');
}

function isObject(value) {
	return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function equal(left, right) {
	return canonicalJson(left) === canonicalJson(right);
}

function failure(message) {
	throw new Error(
		`Notebook person art freshness gate failed: ${message}. Run npm run notebook:people to rebuild the runtime assets from the approved release.`,
	);
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

function expectedRecord(
	entry,
	releaseManifest,
	releaseManifestHash,
	releaseLabel,
) {
	return {
		npc_id: entry.subject?.npc_id,
		real_name: entry.subject?.name,
		display_name: entry.subject?.name,
		approval_status: 'approved',
		provenance: {
			release_manifest: releaseLabel,
			release_manifest_sha256: releaseManifestHash,
			release_id: releaseManifest.release_id,
			job_id: entry.job_id,
			input_record_sha256: entry.subject?.input_record_sha256,
			review_id: entry.approval?.review_id,
			portrait: {
				master_path: entry.art?.portrait?.master_path,
				master_sha256: entry.art?.portrait?.sha256,
				source_candidate_path: entry.art?.portrait?.source_candidate_path,
				source_raw_path: entry.art?.portrait?.source_raw_path,
				source_raw_sha256: entry.art?.portrait?.source_raw_sha256,
			},
			marker: {
				master_path: entry.art?.marker?.master_path,
				master_sha256: entry.art?.marker?.sha256,
				source_candidate_path: entry.art?.marker?.source_candidate_path,
				source_raw_path: entry.art?.marker?.source_raw_path,
				source_raw_sha256: entry.art?.marker?.source_raw_sha256,
			},
		},
	};
}

function verifyRecord(
	record,
	entry,
	releaseManifest,
	releaseManifestHash,
	releaseLabel,
) {
	const expected = expectedRecord(
		entry,
		releaseManifest,
		releaseManifestHash,
		releaseLabel,
	);
	if (!isObject(record))
		failure(`runtime record for ${entry.subject?.name} is missing`);
	for (const key of [
		'npc_id',
		'real_name',
		'display_name',
		'approval_status',
	]) {
		if (record[key] !== expected[key])
			failure(`runtime record for ${entry.subject?.name} has stale ${key}`);
	}
	if (!equal(record.provenance, expected.provenance))
		failure(
			`runtime provenance for ${entry.subject?.name} does not match the approved release`,
		);
}

export async function verifyNotebookPersonArtFreshness(options = {}) {
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
	if (!(await exists(releaseManifestPath))) {
		return { status: 'skipped', release_present: false };
	}

	const runtimeManifestPath = resolve(
		options.runtimeManifestPath ??
			join(uiRoot, 'static', 'rundale', 'notebook-ui', 'asset-manifest.json'),
	);
	if (!(await exists(runtimeManifestPath)))
		failure(`runtime asset manifest is missing at ${runtimeManifestPath}`);

	const releaseBytes = await readFile(releaseManifestPath);
	let releaseManifest;
	let runtimeManifest;
	try {
		releaseManifest = JSON.parse(releaseBytes);
		runtimeManifest = JSON.parse(await readFile(runtimeManifestPath, 'utf8'));
	} catch (error) {
		failure(`could not parse release or runtime manifest (${error.message})`);
	}
	if (!isObject(releaseManifest) || !Array.isArray(releaseManifest.entries))
		failure('approved release manifest has no entries array');
	const { release_id: releaseId, ...releaseBase } = releaseManifest;
	if (releaseId !== sha256(Buffer.from(canonicalJson(releaseBase))))
		failure('approved release manifest release_id does not bind its content');

	const personArt = runtimeManifest?.assets?.personArt;
	if (!isObject(personArt))
		failure('runtime asset manifest has no personArt binding');
	const releaseManifestHash = sha256(releaseBytes);
	const releaseLabel = portablePath(
		options.releaseManifestLabel ?? relative(repoRoot, releaseManifestPath),
	);
	if (personArt.release_id !== releaseId)
		failure('runtime personArt release_id does not match the approved release');
	if (personArt.release_manifest !== releaseLabel)
		failure(
			'runtime personArt release_manifest path does not match the approved release',
		);
	if (personArt.release_manifest_sha256 !== releaseManifestHash)
		failure(
			'runtime personArt release_manifest_sha256 does not match the approved release',
		);
	if (personArt.approval_status !== 'approved')
		failure('runtime personArt is not marked approved');

	const named = releaseManifest.entries.filter(
		(entry) => entry.subject?.kind === 'npc',
	);
	const fallback = releaseManifest.entries.filter(
		(entry) => entry.subject?.kind === 'fallback',
	);
	if (fallback.length !== 1)
		failure(
			'approved release manifest must contain exactly one fallback record',
		);
	if (
		!Array.isArray(personArt.people) ||
		personArt.people.length !== named.length
	)
		failure(
			'runtime personArt people list does not match the approved release',
		);
	for (const [index, entry] of named.entries())
		verifyRecord(
			personArt.people[index],
			entry,
			releaseManifest,
			releaseManifestHash,
			releaseLabel,
		);
	verifyRecord(
		personArt.fallback,
		fallback[0],
		releaseManifest,
		releaseManifestHash,
		releaseLabel,
	);

	return {
		status: 'fresh',
		release_present: true,
		release_id: releaseId,
	};
}

async function main() {
	const { values } = parseArgs({
		options: {
			'release-manifest': { type: 'string' },
			'runtime-manifest': { type: 'string' },
			'repo-root': { type: 'string' },
		},
	});
	const result = await verifyNotebookPersonArtFreshness({
		releaseManifestPath: values['release-manifest'],
		runtimeManifestPath: values['runtime-manifest'],
		repoRoot: values['repo-root'],
	});
	console.log(
		result.status === 'fresh'
			? `Notebook person art is fresh for approved release ${result.release_id}`
			: 'No approved notebook person art release exists; freshness gate skipped',
	);
}

const isMain =
	process.argv[1] &&
	pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (isMain) await main();
