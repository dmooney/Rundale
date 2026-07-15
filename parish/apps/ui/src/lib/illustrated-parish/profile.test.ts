import { describe, expect, it } from 'vitest';
import { parishProfilePlaceholder } from './profile';

describe('parish profile placeholder', () => {
	it('reserves the concept layout without inventing trust or knowledge', () => {
		expect(parishProfilePlaceholder()).toEqual({
			nearbyTrustSlots: 3,
			profileTrustSlots: 4,
			filledTrustSlots: 0,
			knowledgeNotes: ['not yet recorded'],
		});
	});
});
