<script lang="ts">
	/** The freeform mood string from the NPC. */
	let {
		mood,
		size = '1.1em',
		topLeaves = [],
		activeGates = []
	}: {
		mood: string;
		size?: string;
		/** Top-3 leaf descriptors from structured emotion state; when
		 * present, shown in the hover tooltip for a richer read. */
		topLeaves?: string[];
		/** Active behavioural gate names; shown in the tooltip when any
		 * fire (e.g. "panic_truth") to hint at why the NPC is behaving
		 * atypically. */
		activeGates?: string[];
	} = $props();

	/** Emoji lookup table — maps mood keywords to Unicode emoji. */
	const MOOD_EMOJI: Array<{
		keywords: string[];
		emoji: string;
	}> = [
		// Negative/intense — checked first
		{ keywords: ['angry', 'furious', 'enraged', 'irate'], emoji: '😠' },
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
		{ keywords: ['passionate', 'fervent', 'ardent'], emoji: '😍' },

		// Neutral/cognitive
		{ keywords: ['contemplat', 'thoughtful', 'reflective', 'ponder'], emoji: '🤔' },
		{ keywords: ['determined', 'resolute', 'steadfast'], emoji: '💪' },
		{ keywords: ['alert', 'watchful', 'vigilant', 'attentive'], emoji: '👀' },
		{ keywords: ['calm', 'serene', 'peaceful', 'tranquil'], emoji: '😌' },
		{ keywords: ['content', 'satisfied', 'pleased'], emoji: '🙂' },
		{ keywords: ['restless', 'agitated', 'fidgety'], emoji: '😣' },
		{ keywords: ['tired', 'weary', 'exhausted', 'sleepy'], emoji: '😴' },
		{ keywords: ['stoic', 'guarded', 'reserved', 'neutral'], emoji: '😐' },
		{ keywords: ['curious', 'intrigued', 'interested'], emoji: '🧐' },
		{ keywords: ['shy', 'bashful', 'embarrass'], emoji: '😳' },
		{ keywords: ['proud', 'smug', 'self-satisfied'], emoji: '😏' },
		{ keywords: ['surprised', 'astonished', 'shocked'], emoji: '😲' },
		{ keywords: ['warm'], emoji: '🥰' }
	];

	const FALLBACK = '🙂';

	/** Resolve the mood string to an emoji. */
	function resolve(m: string): string {
		const lower = m.toLowerCase();
		for (const entry of MOOD_EMOJI) {
			if (entry.keywords.some((kw) => lower.includes(kw))) return entry.emoji;
		}
		return FALLBACK;
	}

	let emoji = $derived(resolve(mood));

	// Rich tooltip: mood + top leaves + gates. Falls back to the bare
	// mood string when the backend didn't send structured emotion data
	// (older snapshots, emotion system disabled).
	let tooltip = $derived.by(() => {
		const parts = [mood];
		if (topLeaves.length > 0) parts.push(`(${topLeaves.join(', ')})`);
		if (activeGates.length > 0) parts.push(`— ${activeGates.join(', ')}`);
		return parts.join(' ');
	});
</script>

<span class="mood-emoji" title={tooltip} style:font-size={size}>{emoji}</span>

<style>
	.mood-emoji {
		line-height: 1;
		cursor: default;
	}
</style>
