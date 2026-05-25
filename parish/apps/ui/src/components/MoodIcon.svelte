<script lang="ts">
	/** The freeform mood string from the NPC (used for the title tooltip
	 * and as a fallback emoji source when `emoji` is not provided). */
	let {
		mood,
		emoji,
		size = '1.1em'
	}: { mood: string; emoji?: string; size?: string } = $props();

	/**
	 * Fallback emoji lookup table for callers that pass only `mood`.
	 *
	 * The canonical mood→emoji map lives in `parish-npc/src/mood.rs` and is
	 * surfaced to the frontend as `NpcInfo.mood_emoji` on every IPC payload.
	 * Prefer passing `emoji={npc.mood_emoji}` so the UI honours the
	 * backend's choice — this fallback exists for tests and for any future
	 * call site that doesn't have access to the precomputed value.
	 *
	 * The entries below mirror `mood_emoji` in `parish-npc/src/mood.rs`
	 * (TODO #20 — the two maps had drifted: `angry`, `passionate`,
	 * `restless`, `surprised`, and `warm` returned different emojis from
	 * the Rust map vs this Svelte component). Keep them in sync; the
	 * vitest suite asserts a representative cross-section.
	 */
	const MOOD_EMOJI: Array<{
		keywords: string[];
		emoji: string;
	}> = [
		// Negative/intense — checked first
		{ keywords: ['angry', 'furious', 'enraged', 'irate'], emoji: '😡' },
		{ keywords: ['afraid', 'fearful', 'terrified', 'scared'], emoji: '😨' },
		{ keywords: ['anxious', 'nervous', 'worried', 'uneasy'], emoji: '😰' },
		{ keywords: ['sad', 'grief', 'mournful', 'sorrowful'], emoji: '😢' },
		{ keywords: ['melanchol', 'wistful', 'nostalgic', 'pensive'], emoji: '😔' },
		{ keywords: ['irritat', 'frustrat', 'annoyed', 'grumpy'], emoji: '😤' },
		{ keywords: ['suspicious', 'wary', 'distrustful'], emoji: '🤨' },

		// Positive
		{ keywords: ['joy', 'elated', 'ecstatic', 'delighted'], emoji: '😄' },
		{ keywords: ['cheerful', 'jovial', 'merry', 'jolly'], emoji: '😊' },
		{ keywords: ['friendly', 'welcoming', 'hospitable'], emoji: '🤗' },
		{ keywords: ['amused', 'laughing', 'mirthful'], emoji: '😆' },
		{ keywords: ['passionate', 'fervent', 'ardent'], emoji: '🔥' },

		// Neutral/cognitive
		{ keywords: ['contemplat', 'thoughtful', 'reflective', 'ponder'], emoji: '🤔' },
		{ keywords: ['determined', 'resolute', 'steadfast'], emoji: '💪' },
		{ keywords: ['alert', 'watchful', 'vigilant', 'attentive'], emoji: '👀' },
		{ keywords: ['calm', 'serene', 'peaceful', 'tranquil'], emoji: '😌' },
		{ keywords: ['content', 'satisfied', 'pleased'], emoji: '🙂' },
		{ keywords: ['restless', 'agitated', 'fidgety'], emoji: '😟' },
		{ keywords: ['tired', 'weary', 'exhausted', 'sleepy'], emoji: '😴' },
		{ keywords: ['stoic', 'guarded', 'reserved', 'neutral'], emoji: '😐' },
		{ keywords: ['curious', 'intrigued', 'interested'], emoji: '🧐' },
		{ keywords: ['shy', 'bashful', 'embarrass'], emoji: '😳' },
		{ keywords: ['proud', 'smug', 'self-satisfied'], emoji: '😏' },
		{ keywords: ['surprised', 'astonished', 'shocked'], emoji: '😮' },
		{ keywords: ['warm'], emoji: '🤗' }
	];

	const FALLBACK = '🙂';

	function resolve(m: string): string {
		const lower = m.toLowerCase();
		for (const entry of MOOD_EMOJI) {
			if (entry.keywords.some((kw) => lower.includes(kw))) return entry.emoji;
		}
		return FALLBACK;
	}

	// Prefer the backend-computed emoji when present — it is the single
	// source of truth from `parish-npc::mood::mood_emoji`. Fall back to
	// the local map only when the prop is omitted (tests, legacy callers).
	let rendered = $derived(emoji ?? resolve(mood));
</script>

<span class="mood-emoji" title={mood} style:font-size={size}>{rendered}</span>

<style>
	.mood-emoji {
		line-height: 1;
		cursor: default;
	}
</style>
