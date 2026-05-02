// TypeScript mirrors of crates/parish-core/src/editor/types.rs
// All fields in snake_case matching Rust serde defaults.

export interface ModSummary {
	id: string;
	name: string;
	title: string | null;
	version: string;
	description: string;
	path: string;
}

export interface EditorManifest {
	id: string;
	name: string;
	title: string | null;
	version: string;
	description: string;
	start_date: string;
	start_location: number;
	period_year: number;
}

export interface IntelligenceFileEntry {
	verbal: number;
	analytical: number;
	emotional: number;
	practical: number;
	wisdom: number;
	creative: number;
}

export interface ScheduleFileEntry {
	start_hour: number;
	end_hour: number;
	location: number;
	activity: string;
	cuaird?: boolean;
}

export interface ScheduleVariantFileEntry {
	season?: string | null;
	day_type?: string | null;
	entries: ScheduleFileEntry[];
}

export interface RelationshipFileEntry {
	target_id: number;
	kind: string;
	strength: number;
}

export interface NpcFileEntry {
	id: number;
	name: string;
	brief_description?: string | null;
	age: number;
	occupation: string;
	personality: string;
	intelligence?: IntelligenceFileEntry | null;
	home: number;
	workplace: number | null;
	mood: string;
	schedule?: ScheduleFileEntry[] | null;
	seasonal_schedule?: ScheduleVariantFileEntry[] | null;
	relationships: RelationshipFileEntry[];
	knowledge: string[];
}

export interface NpcFile {
	npcs: NpcFileEntry[];
}

export type Hazard = 'none' | 'flood' | 'lakeshore' | 'exposed';

export interface Connection {
	target: number;
	path_description: string;
	hazard?: Hazard;
}

export type GeoKind = 'real' | 'manual' | 'fictional';

export interface RelativeRef {
	anchor: number;
	dnorth_m: number;
	deast_m: number;
}

export interface LocationData {
	id: number;
	name: string;
	description_template: string;
	indoor: boolean;
	public: boolean;
	connections: Connection[];
	lat: number;
	lon: number;
	associated_npcs: number[];
	mythological_significance?: string | null;
	aliases: string[];
	geo_kind?: GeoKind;
	relative_to?: RelativeRef | null;
	geo_source?: string | null;
}

export interface AnachronismEntry {
	term: string;
	category?: string | null;
	origin_year?: number | null;
	note: string;
}

export interface AnachronismData {
	context_alert_prefix: string;
	context_alert_suffix: string;
	terms: AnachronismEntry[];
}

export interface FestivalDef {
	name: string;
	month: number;
	day: number;
	description: string;
}

export interface EncounterTable {
	[key: string]: string;
}

export interface ValidationIssue {
	category: string;
	severity: 'error' | 'warning';
	doc: string;
	field_path: string;
	message: string;
	context?: string | null;
}

export interface ValidationReport {
	errors: ValidationIssue[];
	warnings: ValidationIssue[];
}

export interface EditorModSnapshot {
	mod_path: string;
	manifest: EditorManifest;
	npcs: NpcFile;
	locations: LocationData[];
	festivals: FestivalDef[];
	encounters: EncounterTable;
	anachronisms: AnachronismData;
	validation: ValidationReport;
}

export interface EditorSaveResponse {
	saved: boolean;
	validation: ValidationReport;
}

export type EditorDoc = 'manifest' | 'npcs' | 'world' | 'festivals' | 'encounters' | 'anachronisms';

export type EditorTab = 'mods' | 'npcs' | 'locations' | 'validator' | 'saves';

// ── Save inspector ───────────────────────────────────────────────────────────

export interface SaveFileSummary {
	path: string;
	filename: string;
	file_size: string;
	branch_count: number;
}

export interface BranchSummary {
	id: number;
	name: string;
	parent_branch_id: number | null;
	parent_branch_name: string | null;
	created_at: string;
	snapshot_count: number;
}

export interface SnapshotSummary {
	id: number;
	game_time: string;
	real_time: string;
}

export interface SnapshotDetail {
	id: number;
	branch_id: number;
	game_time: string;
	real_time: string;
	world_state: unknown;
}
