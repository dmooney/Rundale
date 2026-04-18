import { describe, expect, it } from 'vitest';

import { SLASH_COMMANDS, filterCommands } from './slash-commands';

describe('slash command registry', () => {
	it('includes /unexplored in the autocomplete list', () => {
		expect(SLASH_COMMANDS).toContainEqual({
			command: '/unexplored',
			description: 'Reveal or hide all unexplored locations',
			hasArgs: true
		});
	});

	it('returns /unexplored for the /un prefix', () => {
		expect(filterCommands('un')).toEqual([
			{
				command: '/unexplored',
				description: 'Reveal or hide all unexplored locations',
				hasArgs: true
			}
		]);
	});
});
