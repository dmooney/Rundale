/** Reaction palette definition for player↔NPC emoji reactions. */

export interface ReactionDef {
	emoji: string;
	description: string;
	key: string;
}

/** Period-appropriate gestures mapped to emoji.
 *  UI shows the emoji; NPC context receives the description string. */
export const REACTION_PALETTE: ReactionDef[] = [
	{ emoji: '😊', description: 'smiled warmly', key: '1' },
	{ emoji: '😠', description: 'looked angry', key: '2' },
	{ emoji: '😢', description: 'looked sorrowful', key: '3' },
	{ emoji: '😳', description: 'looked startled', key: '4' },
	{ emoji: '🤔', description: 'looked thoughtful', key: '5' },
	{ emoji: '😏', description: 'smirked knowingly', key: '6' },
	{ emoji: '👀', description: 'raised an eyebrow', key: '7' },
	{ emoji: '🤫', description: 'made a hushing gesture', key: '8' },
	{ emoji: '😂', description: 'laughed heartily', key: '9' },
	{ emoji: '🙄', description: 'rolled their eyes', key: '0' },
	{ emoji: '🍺', description: 'raised a glass', key: '-' },
	{ emoji: '✝️', description: 'crossed themselves', key: '=' }
];
