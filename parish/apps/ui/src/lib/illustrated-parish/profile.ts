export interface ParishProfilePlaceholder {
	nearbyTrustSlots: number;
	profileTrustSlots: number;
	filledTrustSlots: number;
	knowledgeNotes: readonly string[];
}

/**
 * Issue #1630 reserves the concept-art profile layout without manufacturing
 * relationship or knowledge state that the engine does not currently expose.
 */
export function parishProfilePlaceholder(): ParishProfilePlaceholder {
	return {
		nearbyTrustSlots: 3,
		profileTrustSlots: 4,
		filledTrustSlots: 0,
		knowledgeNotes: ['not yet recorded'],
	};
}
