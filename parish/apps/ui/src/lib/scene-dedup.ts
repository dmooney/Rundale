/**
 * Scene description deduplication logic.
 *
 * Tracks the last-seen location name and only returns true when the location
 * has changed. This prevents the text log from being cluttered with duplicate
 * scene descriptions on idle turns (non-movement player inputs).
 *
 * The `look` command is handled separately via the text-log IPC handler
 * (it's not a world update, so it doesn't go through this deduplicator).
 */

export class SceneDeduplicator {
	private lastLocationName: string | null = null;

	/**
	 * Check if the location has changed and update internal state.
	 * Returns true if the location is new or has changed from the last seen value.
	 * Returns false if the location is the same as the last seen value.
	 *
	 * @param currentLocationName - The current location name from the world snapshot
	 * @returns true if the location is new or changed, false otherwise
	 */
	shouldShowDescription(currentLocationName: string): boolean {
		if (this.lastLocationName !== currentLocationName) {
			this.lastLocationName = currentLocationName;
			return true;
		}
		return false;
	}

	/**
	 * Reset the deduplicator (useful for loading a new save/session).
	 */
	reset(): void {
		this.lastLocationName = null;
	}
}
