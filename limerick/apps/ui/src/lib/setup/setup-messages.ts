export const INITIAL_SETUP_MESSAGE = 'Preparing the storyteller...';
export const SETUP_START_MESSAGE = 'Starting inference provider setup...';

export function compactSetupMessages(nextMessages: string[]): string[] {
	return nextMessages.filter((msg) => msg.trim().length > 0).slice(-80);
}

export function formatSetupStatusMessage(message: string): string {
	const trimmed = message.trim();
	if (message === SETUP_START_MESSAGE) {
		return 'Opening the parish ledger (starting inference provider setup)...';
	}
	if (trimmed === 'pulling manifest') {
		return "Reading the storyteller's table of contents (Ollama: pulling manifest). The first download can take a few minutes.";
	}
	if (trimmed === 'verifying sha256 digest') {
		return 'Checking the wax seals (Ollama: verifying sha256 digest)...';
	}
	if (trimmed === 'writing manifest') {
		return 'Filing the table of contents in the parish ledger (Ollama: writing manifest)...';
	}
	if (trimmed === 'success') {
		return 'The parcels are in order (Ollama: success).';
	}

	const forced = message.match(/^Forcing a fresh download of '(.+)'\.$/);
	if (forced) {
		return `A clean slate it is: forcing a fresh download of '${forced[1]}'. This big fetch is a one-time thing unless you force it again.`;
	}

	const clearing = message.match(
		/^Clearing the local copy of '(.+)' before fetching it again\.\.\.$/,
	);
	if (clearing) {
		return `Sweeping out the old local copy of '${clearing[1]}' before fetching it again...`;
	}

	const removed = message.match(
		/^Local copy of '(.+)' removed\. Fetching a fresh copy\.\.\.$/,
	);
	if (removed) {
		return `The old '${removed[1]}' copy has left the parish. Fetching a fresh one now...`;
	}

	const missing = message.match(
		/^No local copy of '(.+)' was present\. Fetching it now\.\.\.$/,
	);
	if (missing) {
		return `No local copy of '${missing[1]}' turned up in the ledger. Fetching it now...`;
	}

	const fetching = message.match(
		/^Fetching the storyteller's book of tales \('(.+)'\)\.\.\.$/,
	);
	if (fetching) {
		return `Fetching the storyteller's book of tales ('${fetching[1]}'). This is the big one-time model download; later starts should be much quicker...`;
	}

	return message;
}

export function sentenceBreakParts(
	message: string,
): Array<{ text: string; breakAfter: boolean }> {
	const parts = message.split(/(?<=\.)\s+/).filter((part) => part.length > 0);
	return parts.map((text, index) => ({
		text,
		breakAfter: index < parts.length - 1,
	}));
}

export function isLongSetupWaitMessage(message: string): boolean {
	const lower = message.toLowerCase();
	return (
		lower.includes('pulling manifest') ||
		lower.includes('book of tales') ||
		lower.includes('fresh download') ||
		lower.includes('fresh copy') ||
		lower.includes('one-time model download') ||
		lower.includes('local copy') ||
		lower.includes('download')
	);
}
