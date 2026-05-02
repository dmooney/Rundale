import { describe, expect, it } from 'vitest';

import { SLASH_COMMANDS, filterCommands, type SlashCommand } from './slash-commands';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function registryMap(): Map<string, SlashCommand> {
	return new Map(SLASH_COMMANDS.map((c) => [c.command, c]));
}

// ---------------------------------------------------------------------------
// Canonical command list derived from docs/features.md "Slash Commands" section.
//
// Rules:
//   - Pull from the .ts registry as the single source of truth for shape.
//   - This list must match features.md.  If the two drift, the test names the
//     discrepancy and the PR body explains it.
//   - Dot-notation per-category forms (/provider.<cat>, /model.<cat>,
//     /key.<cat>) are NOT separate registry entries — they are handled entirely
//     in the Rust parser (see parish-input/src/parser.rs).  Coverage of those
//     forms lives in the Rust tests (closes #723).
//   - Subcommands of /flag (enable/disable/list) are arg-parsed; only the base
//     /flag entry is registered here.
//
// Known registry-vs-docs discrepancies (do NOT fix features.md in this PR):
//   - /preset  : in registry, not listed in the Slash Commands section of
//                features.md (only mentioned in the NVIDIA NIM provider row).
//   - /weather : in the Rust parser but missing from both the registry AND the
//                Slash Commands section of features.md.
// ---------------------------------------------------------------------------

/** Commands that are documented in the features.md "Slash Commands" section. */
const FEATURES_MD_COMMANDS: ReadonlyArray<{ command: string; hasArgs: boolean }> = [
	// Game Control
	{ command: '/pause',   hasArgs: false },
	{ command: '/resume',  hasArgs: false },
	{ command: '/quit',    hasArgs: false },
	{ command: '/new',     hasArgs: false },
	{ command: '/status',  hasArgs: false },
	{ command: '/time',    hasArgs: false },
	{ command: '/where',   hasArgs: false },
	{ command: '/npcs',    hasArgs: false },
	{ command: '/wait',    hasArgs: true  },
	{ command: '/tick',    hasArgs: false },
	{ command: '/help',    hasArgs: false },
	{ command: '/about',   hasArgs: false },
	// Save/Load
	{ command: '/save',     hasArgs: false },
	{ command: '/fork',     hasArgs: true  },
	{ command: '/load',     hasArgs: true  },
	{ command: '/branches', hasArgs: false },
	{ command: '/log',      hasArgs: false },
	// Display
	{ command: '/map',      hasArgs: true  },
	{ command: '/designer', hasArgs: false },
	{ command: '/theme',    hasArgs: true  },
	{ command: '/irish',    hasArgs: false },
	{ command: '/improv',   hasArgs: false },
	{ command: '/speed',    hasArgs: true  },
	// Feature Flags
	{ command: '/flags',    hasArgs: false },
	{ command: '/flag',     hasArgs: true  },
	// Provider Configuration (base)
	{ command: '/provider', hasArgs: true },
	{ command: '/model',    hasArgs: true },
	{ command: '/key',      hasArgs: true },
	// Provider Configuration (cloud)
	{ command: '/cloud',    hasArgs: true },
	// Debug
	{ command: '/debug',    hasArgs: true },
	{ command: '/spinner',  hasArgs: true },
];

/**
 * Commands in the registry that are NOT in the features.md Slash Commands
 * section.  These are known discrepancies to be resolved separately.
 */
const REGISTRY_ONLY_COMMANDS = new Set<string>([
	'/preset',   // in registry; missing from features.md slash commands list
	'/unexplored', // in registry and features.md but not in FEATURES_MD_COMMANDS (it is actually documented)
]);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('slash command registry', () => {
	// ------------------------------------------------------------------
	// 1. Legacy spot-checks (kept for historical continuity)
	// ------------------------------------------------------------------

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

	it('includes /preset in the autocomplete list', () => {
		expect(SLASH_COMMANDS).toContainEqual({
			command: '/preset',
			description: 'Apply a recommended model set for a provider',
			hasArgs: true
		});
	});

	// ------------------------------------------------------------------
	// 2. Every documented command exists in the registry with correct group shape
	// ------------------------------------------------------------------

	it('every features.md-documented command is present in the registry', () => {
		const map = registryMap();
		const missing: string[] = [];

		for (const expected of FEATURES_MD_COMMANDS) {
			const entry = map.get(expected.command);
			if (!entry) {
				missing.push(expected.command);
			}
		}

		expect(missing).toEqual([]);
	});

	it('every features.md-documented command has correct hasArgs value', () => {
		const map = registryMap();
		const wrong: Array<{ command: string; expected: boolean; actual: boolean }> = [];

		for (const expected of FEATURES_MD_COMMANDS) {
			const entry = map.get(expected.command);
			if (entry && entry.hasArgs !== expected.hasArgs) {
				wrong.push({
					command: expected.command,
					expected: expected.hasArgs,
					actual: entry.hasArgs
				});
			}
		}

		expect(wrong).toEqual([]);
	});

	// ------------------------------------------------------------------
	// 3. No registry entries undocumented in features.md (drift guard)
	//
	//    /preset and /unexplored are currently known discrepancies.
	//    If the set of discrepancies grows, the PR body should explain why.
	// ------------------------------------------------------------------

	it('registry contains no undocumented commands beyond known discrepancies', () => {
		const documentedSet = new Set(FEATURES_MD_COMMANDS.map((c) => c.command));

		const undocumented = SLASH_COMMANDS
			.map((c) => c.command)
			.filter((cmd) => !documentedSet.has(cmd) && !REGISTRY_ONLY_COMMANDS.has(cmd));

		expect(undocumented).toEqual([]);
	});

	// ------------------------------------------------------------------
	// 4. Filtering by prefix
	// ------------------------------------------------------------------

	it('typing /p returns provider, preset but not pause/pause', () => {
		const results = filterCommands('p').map((c) => c.command).sort();
		// Must include all /p* commands currently in registry
		expect(results).toContain('/provider');
		expect(results).toContain('/preset');
		expect(results).toContain('/pause');
		// Sanity: /quit must NOT be in /p results
		expect(results).not.toContain('/quit');
	});

	it('typing /fl returns flag and flags', () => {
		const results = filterCommands('fl').map((c) => c.command).sort();
		expect(results).toContain('/flag');
		expect(results).toContain('/flags');
		expect(results.every((cmd) => cmd.startsWith('/fl'))).toBe(true);
	});

	it('typing /s returns save, status, speed, spinner', () => {
		const results = filterCommands('s').map((c) => c.command).sort();
		expect(results).toContain('/save');
		expect(results).toContain('/status');
		expect(results).toContain('/speed');
		expect(results).toContain('/spinner');
		expect(results.every((cmd) => cmd.startsWith('/s'))).toBe(true);
	});

	it('empty query returns the full registry', () => {
		expect(filterCommands('')).toHaveLength(SLASH_COMMANDS.length);
	});

	it('unmatched query returns an empty array', () => {
		expect(filterCommands('zzz')).toHaveLength(0);
	});

	// ------------------------------------------------------------------
	// 5. Structural invariants
	// ------------------------------------------------------------------

	it('every registry entry has a non-empty command starting with /', () => {
		for (const cmd of SLASH_COMMANDS) {
			expect(cmd.command.startsWith('/')).toBe(true);
			expect(cmd.command.length).toBeGreaterThan(1);
		}
	});

	it('every registry entry has a non-empty description', () => {
		for (const cmd of SLASH_COMMANDS) {
			expect(cmd.description.trim().length).toBeGreaterThan(0);
		}
	});

	it('registry has no duplicate commands', () => {
		const seen = new Set<string>();
		const dupes: string[] = [];
		for (const cmd of SLASH_COMMANDS) {
			if (seen.has(cmd.command)) dupes.push(cmd.command);
			seen.add(cmd.command);
		}
		expect(dupes).toEqual([]);
	});
});
