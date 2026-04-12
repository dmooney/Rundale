/// Static list of slash commands for autocomplete, mirroring the backend's
/// `parse_system_command` in `crates/parish-core/src/input/mod.rs`.

export interface SlashCommand {
	command: string;
	description: string;
	/** If true, the command accepts an argument after a space. */
	hasArgs: boolean;
}

export const SLASH_COMMANDS: SlashCommand[] = [
	{ command: '/help', description: 'Show available commands', hasArgs: false },
	{ command: '/status', description: 'Where am I?', hasArgs: false },
	{ command: '/save', description: 'Save game', hasArgs: false },
	{ command: '/load', description: 'Load a saved branch', hasArgs: true },
	{ command: '/fork', description: 'Fork a new timeline branch', hasArgs: true },
	{ command: '/branches', description: 'List save branches', hasArgs: false },
	{ command: '/log', description: 'Show snapshot history', hasArgs: false },
	{ command: '/pause', description: 'Hold time still', hasArgs: false },
	{ command: '/resume', description: 'Let time flow again', hasArgs: false },
	{ command: '/speed', description: 'Show or change game speed', hasArgs: true },
	{ command: '/irish', description: 'Toggle language hints sidebar', hasArgs: false },
	{ command: '/improv', description: 'Toggle improv craft mode', hasArgs: false },
	{ command: '/about', description: 'About the game', hasArgs: false },
	{ command: '/provider', description: 'Show or change LLM provider', hasArgs: true },
	{ command: '/model', description: 'Show or change LLM model', hasArgs: true },
	{ command: '/key', description: 'Show or change API key', hasArgs: true },
	{ command: '/cloud', description: 'Cloud provider settings', hasArgs: true },
	{ command: '/debug', description: 'Debug panel', hasArgs: true },
	{ command: '/spinner', description: 'Show loading spinner', hasArgs: true },
	{ command: '/map', description: 'Show the parish map', hasArgs: false },
	{ command: '/npcs', description: "Who's here?", hasArgs: false },
	{ command: '/time', description: 'What time is it?', hasArgs: false },
	{ command: '/where', description: 'Where am I? (alias for /status)', hasArgs: false },
	{ command: '/wait', description: 'Wait N minutes (default 15)', hasArgs: true },
	{ command: '/tick', description: 'Advance NPC schedules', hasArgs: false },
	{ command: '/new', description: 'Start a new game', hasArgs: false },
	{ command: '/quit', description: 'Take your leave', hasArgs: false },
	{ command: '/flag', description: 'Feature flags: enable/disable/list', hasArgs: true },
	{ command: '/flags', description: 'List all feature flags', hasArgs: false }
];

/// Filter commands by prefix query (the text after `/`).
export function filterCommands(query: string): SlashCommand[] {
	if (query === '') return SLASH_COMMANDS;
	const lower = query.toLowerCase();
	return SLASH_COMMANDS.filter((cmd) => cmd.command.slice(1).toLowerCase().startsWith(lower));
}
