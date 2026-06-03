// Repo-wide Prettier config for docs/data formats (Markdown, JSON, YAML).
// The game frontend (parish/apps/ui) has its own .prettierrc and is excluded
// here via .prettierignore. Matches the frontend's tab/quote conventions so a
// file moved between scopes formats identically.
/** @type {import("prettier").Config} */
export default {
	useTabs: true,
	singleQuote: true,
	trailingComma: 'all',
	// Don't reflow prose — keep line breaks as authored (smaller, semantic diffs).
	proseWrap: 'preserve',
	overrides: [
		{
			// JSON/YAML have no tabs convention; 2-space is the ecosystem norm.
			files: ['*.json', '*.jsonc', '*.yaml', '*.yml'],
			options: { useTabs: false, tabWidth: 2, singleQuote: false },
		},
		{
			files: ['*.md'],
			options: { useTabs: false, tabWidth: 2 },
		},
	],
};
