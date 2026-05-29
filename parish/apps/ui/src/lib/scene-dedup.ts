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
	private suppressNext = false;

	/**
	 * Check if the location has changed and update internal state.
	 * Returns true if the location is new or has changed from the last seen value.
	 * Returns false if the location is the same as the last seen value.
	 *
	 * If a `location` text-log entry was already shown for this transition (see
	 * {@link suppressNextDescription}), the changed-location case returns false
	 * once — `lastLocationName` is still advanced so later idle updates stay
	 * deduped — preventing the movement scene from printing twice.
	 *
	 * @param currentLocationName - The current location name from the world snapshot
	 * @returns true if the location is new or changed, false otherwise
	 */
	shouldShowDescription(currentLocationName: string): boolean {
		const changed = this.lastLocationName !== currentLocationName;
		this.lastLocationName = currentLocationName;
		if (this.suppressNext) {
			this.suppressNext = false;
			return false;
		}
		return changed;
	}

	/**
	 * Mark that the scene description has already been rendered for the current
	 * transition by another channel — i.e. a `text-log` entry with
	 * `subtype: "location"`. Movement is the only path that emits such an entry,
	 * and it always emits a `world-update` immediately afterward, so the next
	 * {@link shouldShowDescription} call (from that world-update) is suppressed
	 * to avoid a duplicate, shorter scene line.
	 */
	suppressNextDescription(): void {
		this.suppressNext = true;
	}

	/**
	 * Reset the deduplicator (useful for loading a new save/session).
	 */
	reset(): void {
		this.lastLocationName = null;
		this.suppressNext = false;
	}
}
