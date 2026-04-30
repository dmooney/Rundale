export type StreamChunkResult = {
	chunk: string | null;
	rest: string;
};

type BufferedWord = {
	text: string;
	end: number;
};

const WORD_REGEX = /\s*\S+(?:\s+|$)/gu;
const BASE_CHUNK_DELAY_MS = 120;
const CLAUSE_PAUSE_MS = 90;
const SENTENCE_PAUSE_MS = 190;

function tokenizeBufferedWords(buffer: string, flush: boolean): { words: BufferedWord[]; consumed: number } {
	const words: BufferedWord[] = [];
	let consumed = 0;
	let match: RegExpExecArray | null;

	WORD_REGEX.lastIndex = 0;
	while ((match = WORD_REGEX.exec(buffer)) !== null) {
		const text = match[0];
		const end = match.index + text.length;
		const endsWithWhitespace = /\s/u.test(text[text.length - 1] ?? '');
		if (!flush && end === buffer.length && !endsWithWhitespace) {
			break;
		}
		words.push({ text, end });
		consumed = end;
	}

	return { words, consumed };
}

export function takeNextStreamChunk(buffer: string, flush = false): StreamChunkResult {
	if (buffer.length === 0) {
		return { chunk: null, rest: buffer };
	}

	const { words, consumed } = tokenizeBufferedWords(buffer, flush);
	if (words.length === 0) {
		return flush ? { chunk: buffer, rest: '' } : { chunk: null, rest: buffer };
	}

	const chunkEnd = words[0]?.end ?? consumed;
	return {
		chunk: buffer.slice(0, chunkEnd),
		rest: buffer.slice(chunkEnd)
	};
}

export function getStreamChunkDelayMs(chunk: string): number {
	const trimmed = chunk.trimEnd();
	let delay = BASE_CHUNK_DELAY_MS;

	if (/[.?!…]['")\]]*$/u.test(trimmed)) {
		delay += SENTENCE_PAUSE_MS;
	} else if (/[,;:]['")\]]*$/u.test(trimmed)) {
		delay += CLAUSE_PAUSE_MS;
	}

	return delay;
}
