import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import MoodIcon from './MoodIcon.svelte';

describe('MoodIcon', () => {
	it('renders an emoji for a known mood', () => {
		const { container } = render(MoodIcon, { props: { mood: 'cheerful' } });
		const span = container.querySelector('.mood-emoji');
		expect(span).toBeTruthy();
		expect(span?.textContent).toBe('😊');
	});

	it('renders fallback emoji for an unknown mood', () => {
		const { container } = render(MoodIcon, { props: { mood: 'zzz_unknown_zzz' } });
		const span = container.querySelector('.mood-emoji');
		expect(span).toBeTruthy();
		expect(span?.textContent).toBe('🙂');
	});

	it('sets the title attribute to the mood string', () => {
		const { container } = render(MoodIcon, { props: { mood: 'anxious' } });
		const span = container.querySelector('.mood-emoji');
		expect(span?.getAttribute('title')).toBe('anxious');
	});

	it('renders different emoji for different moods', () => {
		const { container: c1 } = render(MoodIcon, { props: { mood: 'angry' } });
		const { container: c2 } = render(MoodIcon, { props: { mood: 'joyful' } });
		const e1 = c1.querySelector('.mood-emoji')?.textContent;
		const e2 = c2.querySelector('.mood-emoji')?.textContent;
		expect(e1).toBe('😠');
		expect(e2).toBe('😄');
		expect(e1).not.toBe(e2);
	});

	it('matches mood keywords as substrings', () => {
		// "contemplative" should match the "contemplat" keyword
		const { container } = render(MoodIcon, { props: { mood: 'contemplative' } });
		const span = container.querySelector('.mood-emoji');
		expect(span?.textContent).toBe('🤔');
	});

	it('matches moods case-insensitively', () => {
		const { container: lower } = render(MoodIcon, { props: { mood: 'angry' } });
		const { container: upper } = render(MoodIcon, { props: { mood: 'ANGRY' } });
		expect(lower.querySelector('.mood-emoji')?.textContent).toBe('😠');
		expect(upper.querySelector('.mood-emoji')?.textContent).toBe('😠');
	});

	it('renders all mood categories to an emoji', () => {
		const moods = [
			'angry', 'afraid', 'anxious', 'sad', 'melancholy', 'irritated', 'suspicious',
			'joyful', 'cheerful', 'friendly', 'amused', 'passionate',
			'contemplative', 'determined', 'alert', 'calm', 'content',
			'restless', 'tired', 'stoic', 'curious', 'shy', 'proud', 'surprised', 'warm'
		];
		for (const mood of moods) {
			const { container } = render(MoodIcon, { props: { mood } });
			const emoji = container.querySelector('.mood-emoji')?.textContent;
			expect(emoji, `mood "${mood}" should render something`).toBeTruthy();
		}
	});

	it('renders unique emoji for moods that are not the fallback', () => {
		// "content" uses 🙂 which is also the fallback — that's intentional.
		// All other moods should differ from fallback.
		const moods = [
			'angry', 'afraid', 'anxious', 'sad', 'melancholy', 'irritated', 'suspicious',
			'joyful', 'cheerful', 'friendly', 'amused', 'passionate',
			'contemplative', 'determined', 'alert', 'calm',
			'restless', 'tired', 'stoic', 'curious', 'shy', 'proud', 'surprised', 'warm'
		];
		for (const mood of moods) {
			const { container } = render(MoodIcon, { props: { mood } });
			const emoji = container.querySelector('.mood-emoji')?.textContent;
			expect(emoji, `mood "${mood}" should not be fallback`).not.toBe('🙂');
		}
	});
});
