/**
 * Sunrise/sunset calculator — pure TypeScript, no external dependencies.
 *
 * Uses the simplified NOAA solar position algorithm (accurate to ~2 minutes).
 * Defaults to central Ireland coordinates when geolocation is unavailable.
 */

/** Central Ireland — used as fallback when geolocation is unavailable. */
export const IRELAND_LAT = 53.4;
export const IRELAND_LON = -8.0;

/** Switching threshold in milliseconds (30 minutes). */
const THRESHOLD_MS = 30 * 60 * 1000;

function degToRad(d: number): number {
	return (d * Math.PI) / 180;
}

function radToDeg(r: number): number {
	return (r * 180) / Math.PI;
}

/**
 * Computes sunrise and sunset times (as Date objects in local time) for the
 * given latitude, longitude, and calendar date.
 *
 * Returns the same Date for both if the location experiences polar day/night.
 */
export function getSunTimes(
	lat: number,
	lon: number,
	date: Date
): { sunrise: Date; sunset: Date } {
	// Julian date
	const JD = date.getTime() / 86_400_000 + 2_440_587.5;
	// Days since J2000.0
	const n = JD - 2_451_545.0;

	// Mean longitude and mean anomaly (degrees)
	const L = ((280.46 + 0.9856474 * n) % 360 + 360) % 360;
	const g = degToRad(((357.528 + 0.9856003 * n) % 360 + 360) % 360);

	// Ecliptic longitude (degrees → radians)
	const lambda = degToRad(L + 1.915 * Math.sin(g) + 0.02 * Math.sin(2 * g));

	// Obliquity of the ecliptic (radians)
	const epsilon = degToRad(23.439 - 0.0000004 * n);

	// Right ascension (degrees)
	const RA =
		radToDeg(Math.atan2(Math.cos(epsilon) * Math.sin(lambda), Math.cos(lambda))) % 360;

	// Declination (radians)
	const sinDec = Math.sin(epsilon) * Math.sin(lambda);
	const dec = Math.asin(sinDec);

	// Equation of time (hours)
	const EqT = (L - RA) / 15;

	// Solar noon in UTC hours
	const solarNoonUTC = 12 - lon / 15 - EqT;

	// Hour angle at sunrise/sunset (solar elevation = -0.833° accounts for refraction + disc)
	const cosH =
		(Math.sin(degToRad(-0.833)) - Math.sin(degToRad(lat)) * sinDec) /
		(Math.cos(degToRad(lat)) * Math.cos(dec));

	// Polar day or night — return solar noon for both to avoid NaN
	if (cosH < -1 || cosH > 1) {
		const noon = utcHoursToDate(date, solarNoonUTC);
		return { sunrise: noon, sunset: noon };
	}

	const H = radToDeg(Math.acos(cosH)) / 15; // hours

	return {
		sunrise: utcHoursToDate(date, solarNoonUTC - H),
		sunset: utcHoursToDate(date, solarNoonUTC + H)
	};
}

/** Converts a fractional UTC hour offset for the given calendar day into a Date. */
function utcHoursToDate(date: Date, utcHours: number): Date {
	const d = new Date(date);
	d.setUTCHours(0, 0, 0, 0);
	d.setTime(d.getTime() + utcHours * 3_600_000);
	return d;
}

/**
 * Returns true if the current real-world clock is in "night" mode:
 *   - at or after (sunset + 30 min), OR
 *   - before (sunrise − 30 min).
 */
export function isNightNow(sunrise: Date, sunset: Date): boolean {
	const now = Date.now();
	const nightStart = sunset.getTime() + THRESHOLD_MS;
	const nightEnd = sunrise.getTime() - THRESHOLD_MS;
	// nightEnd may be in the past (yesterday's sunrise); if so, check both sides
	return now >= nightStart || now < nightEnd;
}

/**
 * Returns the Date of the next auto-switch event — whichever of
 * (sunrise − 30 min) or (sunset + 30 min) comes soonest in the future.
 * Returns null if both thresholds are in the past (shouldn't happen in practice).
 */
export function nextSwitchTime(sunrise: Date, sunset: Date): Date | null {
	const now = Date.now();
	const candidates = [
		sunrise.getTime() - THRESHOLD_MS, // switch to light
		sunset.getTime() + THRESHOLD_MS // switch to dark
	]
		.filter((t) => t > now)
		.sort((a, b) => a - b);

	return candidates.length > 0 ? new Date(candidates[0]) : null;
}

/**
 * Attempts to get the user's current coordinates via the browser Geolocation API
 * with a 1.5-second timeout. Falls back to central Ireland on failure.
 */
export async function getUserCoords(): Promise<{ lat: number; lon: number }> {
	return new Promise((resolve) => {
		if (typeof navigator === 'undefined' || !navigator.geolocation) {
			resolve({ lat: IRELAND_LAT, lon: IRELAND_LON });
			return;
		}
		const timer = setTimeout(
			() => resolve({ lat: IRELAND_LAT, lon: IRELAND_LON }),
			1500
		);
		navigator.geolocation.getCurrentPosition(
			(pos) => {
				clearTimeout(timer);
				resolve({ lat: pos.coords.latitude, lon: pos.coords.longitude });
			},
			() => {
				clearTimeout(timer);
				resolve({ lat: IRELAND_LAT, lon: IRELAND_LON });
			},
			{ timeout: 1500, maximumAge: 600_000 }
		);
	});
}
