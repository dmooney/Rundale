// All fields in snake_case matching Rust serde defaults

export type PlayerTaskStatus = 'assigned' | 'in_progress' | 'completed';

export interface PlayerTaskSnapshot {
	id: number;
	description: string;
	assigned_by: number;
	location_id: number;
	status: PlayerTaskStatus;
	assigned_at: string;
	started_at: string | null;
	completed_at: string | null;
	last_matching_action: string | null;
}

export interface WorldSnapshot {
	/** Canonical ID of the player's current location. */
	location_id: number;
	location_name: string;
	location_description: string;
	time_label: string;
	hour: number;
	minute: number;
	weather: string;
	season: string;
	festival: string | null;
	paused: boolean;
	inference_paused: boolean;
	game_epoch_ms: number;
	speed_factor: number;
	name_hints: LanguageHint[];
	day_of_week: string;
	active_tasks: PlayerTaskSnapshot[];
	/** Whether an NPC conversation turn is being processed by the engine.
	 *  Set on the `/api/world-snapshot` resync the reconnect handler fetches so
	 *  the client can re-assert `streamingActive` from authoritative state
	 *  instead of clearing it mid-turn (#1164). `#[serde(default)]` on the Rust
	 *  side, so it may be absent on world-update pushes — treat undefined as
	 *  false. */
	turn_in_flight?: boolean;
}

export interface MapLocation {
	id: string;
	name: string;
	lat: number;
	lon: number;
	adjacent: boolean;
	hops: number;
	indoor?: boolean;
	travel_minutes?: number;
	/** Whether the player has visited this location (false = fog-of-war frontier). */
	visited?: boolean;
}

export interface MapData {
	locations: MapLocation[];
	edges: [string, string][];
	player_location: string;
	/** Edge traversal counts for footprint rendering: [src_id, dst_id, count].
	 *  Rust skips this when empty (`skip_serializing_if = "Vec::is_empty"`). */
	edge_traversals?: [string, string, number][];
	/** Human-readable transport mode label (e.g. "on foot").
	 *  Always serialized by Rust `MapData` (`pub transport_label: String`). */
	transport_label: string;
	/** Machine identifier for the active transport mode (e.g. "walking").
	 *  Always serialized by Rust `MapData` (`pub transport_id: String`). */
	transport_id: string;
}

/** Tooltip data shown when hovering a map location marker.
 *  Used by both MapPanel (minimap) and FullMapOverlay (full map). */
export interface MapTooltipInfo {
	name: string;
	indoor?: boolean;
	travel_minutes?: number;
	visited?: boolean;
}

/** A waypoint along a travel path. */
export interface TravelWaypoint {
	id: string;
	lat: number;
	lon: number;
}

/** Payload for travel-start events (animated travel on the map). */
export interface TravelStartPayload {
	waypoints: TravelWaypoint[];
	duration_minutes: number;
	destination: string;
}

export interface NpcInfo {
	npc_id: number;
	name: string;
	/** Canonical real name, retained for display and compatibility lookups. */
	real_name: string;
	occupation: string;
	mood: string;
	introduced: boolean;
	mood_emoji: string;
}

/**
 * One persistence-gated reconnect snapshot.
 *
 * The server and Tauri command capture these fields under the same canonical
 * state locks. Consumers must validate and commit the envelope as one unit;
 * independently refreshing its children can mix two game contexts.
 */
export interface ReconnectState {
	world: WorldSnapshot;
	map: MapData;
	npcs: NpcInfo[];
	context_epoch: number;
}

export interface ThemePalette {
	bg: string;
	fg: string;
	accent: string;
	panel_bg: string;
	input_bg: string;
	border: string;
	muted: string;
}

export interface LanguageHint {
	word: string;
	pronunciation: string;
	meaning: string | null;
}

export interface ModEntry {
	id: string;
	name: string;
	title: string | null;
	version: string;
	description: string;
	active: boolean;
}

export interface UiConfig {
	hints_label: string;
	default_accent: string;
	splash_text: string;
	active_tile_source: string;
	tile_sources: TileSource[];
	auto_pause_timeout_seconds: number;
	app_icon_url?: string | null;
	favicon_url?: string | null;
	map_overlay?: string | null;
	base_mod_required?: boolean;
}

/** A single map tile source sent from the backend. Mirrors
 *  `parish_core::ipc::TileSourceSnapshot`. */
export interface TileSource {
	id: string;
	label: string;
	url: string;
	tile_size: number;
	minzoom: number;
	maxzoom: number;
	attribution: string;
	raster_saturation: number;
	raster_opacity: number;
	tms: boolean;
}

export interface Reaction {
	emoji: string;
	source: string;
}

export interface TextLogEntry {
	id?: string;
	source: string;
	content: string;
	subtype?: string;
	stream_turn_id?: number;
	streaming?: boolean;
	latest_chunk?: string;
	stream_chunk_id?: number;
	reactions?: Reaction[];
}

export interface StreamTokenPayload {
	/** Player-renderable canonical text. Tier-1 provider candidate tokens are
	 * quarantined by the engine and never cross this protocol boundary (#1834). */
	token: string;
	turn_id: number;
	source: string;
	/** Message id of the `text-log` placeholder this stream fills. Carried so a
	 *  stream that resumes after a WS reconnect can rebind to a reactable
	 *  `textLog` entry even when `StreamManager.reset()` finalized the client's
	 *  earlier presentation state (#1164). Absent for arrival-reaction
	 *  streams (Rust serializes `Option<String>` with `skip_serializing_if`). */
	message_id?: string;
}

export interface StreamTurnEndPayload {
	turn_id: number;
	/** Authoritative disposition of the complete provider response. */
	status: 'completed' | 'failed';
	/** Stable canonical dialogue message identity, when this was an NPC turn. */
	message_id?: string;
	/** Display source for a completed dialogue line. */
	source?: string;
	/** Complete, validated player-renderable text. Never a provider partial. */
	final_text?: string;
	/** Player-visible retry guidance for a failed locally initiated turn. */
	recovery_message?: string;
}

export interface StreamEndPayload {
	hints: LanguageHint[];
}

/** Payload for `dialogue-corrected` events.
 *
 * Emitted by the backend after all post-generation guards run on an NPC turn,
 * only when at least one guard altered the raw model output. The UI must
 * replace the accumulated stream tokens for `turn_id` with `corrected_text`
 * so the player sees the post-guard canonical dialogue (#1552).
 */
export interface DialogueCorrectedPayload {
	turn_id: number;
	corrected_text: string;
	/** Stable message ID from the original placeholder `text-log` event.
	 *  Present when the backend emits it (post-#1552 parity); allows the UI to
	 *  locate the entry by id when `stream_turn_id` has already been cleared. */
	message_id?: string;
}

export interface TextLogPayload {
	/** Unique message id for reaction targeting. Rust always serializes this
	 *  (`#[serde(default)] pub id: String`); `#[serde(default)]` only affects
	 *  deserialization, so the wire payload always carries a (possibly empty) id. */
	id: string;
	stream_turn_id?: number;
	source: string;
	content: string;
	subtype?: string;
}

export interface NpcReactionPayload {
	message_id: string;
	emoji: string;
	source: string;
}

export type WorldUpdatePayload = WorldSnapshot;

export interface LoadingPayload {
	active: boolean;
	spinner?: string;
	phrase?: string;
	color?: [number, number, number];
}

// ── Debug types ─────────────────────────────────────────────────────────────

export interface DebugSnapshot {
	clock: ClockDebug;
	weather: WeatherDebug;
	world: WorldDebug;
	npcs: NpcDebug[];
	tier_summary: TierSummary;
	event_bus: EventBusDebug;
	gossip: GossipDebug;
	conversations: ConversationsDebug;
	events: DebugEvent[];
	inference: InferenceDebug;
	auth: AuthDebug;
}

export interface AuthDebug {
	oauth_enabled: boolean;
	logged_in: boolean;
	provider: string | null;
	display_name: string | null;
	session_id: string | null;
}

/**
 * Response body for `GET /api/auth/status` (web server only).
 *
 * Mirrors `parish_server::auth::AuthStatus` — keep in sync with that struct.
 * Distinct from {@link AuthDebug} (the debug-snapshot shape, which also carries
 * `session_id`); this is the narrower public auth-status payload.
 */
export interface AuthStatus {
	oauth_enabled: boolean;
	logged_in: boolean;
	provider?: string | null;
	display_name?: string | null;
}

export interface ClockDebug {
	game_time: string;
	time_of_day: string;
	season: string;
	festival: string | null;
	weather: string;
	paused: boolean;
	inference_paused: boolean;
	speed_factor: number;
	speed_name: string | null;
	day_of_week: string;
	day_type: string;
	start_game_time: string;
	paused_game_time: string;
	real_elapsed_secs: number;
}

export interface WeatherDebug {
	current: string;
	since: string;
	duration_hours: number;
	min_duration_hours: number;
	last_check_at: string | null;
}

export interface WorldDebug {
	player_location_name: string;
	player_location_id: number;
	location_count: number;
	visited_count: number;
	visited_locations: string[];
	edge_traversals: EdgeTraversalDebug[];
	text_log_tail: string[];
	text_log_len: number;
	locations: LocationDebug[];
	player_name: string | null;
}

export interface EdgeTraversalDebug {
	from_name: string;
	to_name: string;
	count: number;
}

export interface LocationDebug {
	id: number;
	name: string;
	indoor: boolean;
	public: boolean;
	connection_count: number;
	npcs_here: string[];
	visited: boolean;
	edges: GraphEdgeDebug[];
}

export interface GraphEdgeDebug {
	target_id: number;
	target_name: string;
	path_description: string;
	walking_minutes: number;
}

export interface NpcDebug {
	id: number;
	name: string;
	brief_description: string;
	introduced: boolean;
	age: number;
	occupation: string;
	personality: string;
	location_name: string;
	location_id: number;
	home_name: string | null;
	workplace_name: string | null;
	mood: string;
	is_ill: boolean;
	state: string;
	tier: string;
	schedule: ScheduleVariantDebug[];
	relationships: RelationshipDebug[];
	memories: MemoryDebug[];
	long_term_memories: LongTermMemoryDebug[];
	reactions: ReactionDebug[];
	deflated_summary: DeflatedSummaryDebug | null;
	knowledge: string[];
	intelligence: IntelligenceDebug;
	last_activity: string | null;
	knows_player_name: boolean;
}

export interface LongTermMemoryDebug {
	timestamp: string;
	content: string;
	importance: number;
	keywords: string[];
}

export interface ReactionDebug {
	direction: 'PlayerToNpc' | 'NpcToPlayer';
	timestamp: string;
	emoji: string;
	description: string;
	context: string;
}

export interface DeflatedSummaryDebug {
	location_name: string;
	mood: string;
	recent_activity: string[];
	key_relationship_changes: string[];
}

export interface ScheduleVariantDebug {
	season: string | null;
	day_type: string | null;
	is_active: boolean;
	entries: ScheduleEntryDebug[];
}

export interface IntelligenceDebug {
	verbal: number;
	analytical: number;
	emotional: number;
	practical: number;
	wisdom: number;
	creative: number;
}

export interface ScheduleEntryDebug {
	start_hour: number;
	end_hour: number;
	location_name: string;
	activity: string;
	is_current: boolean;
}

export interface RelationshipDebug {
	target_name: string;
	kind: string;
	strength: number;
	history_count: number;
	history: RelationshipEventDebug[];
}

export interface RelationshipEventDebug {
	timestamp: string;
	description: string;
}

export interface MemoryDebug {
	timestamp: string;
	content: string;
	location_name: string;
}

export interface TierSummary {
	tier1_count: number;
	tier2_count: number;
	tier3_count: number;
	tier4_count: number;
	tier1_names: string[];
	tier2_names: string[];
	tier3_names: string[];
	tier4_names: string[];
	tier3_in_flight: boolean;
	last_tier2_tick: string | null;
	last_tier3_tick: string | null;
	last_tier4_tick: string | null;
	introduced_count: number;
	tier2_in_flight: boolean;
	tier3_pending_count: number;
	tier4_recent_events: string[];
}

export interface EventBusDebug {
	subscriber_count: number;
	recent_events: GameEventDebug[];
}

export interface GameEventDebug {
	timestamp: string;
	kind: string;
	summary: string;
}

export interface GossipDebug {
	item_count: number;
	items: GossipItemDebug[];
}

export interface GossipItemDebug {
	id: number;
	content: string;
	source_name: string;
	distortion_level: number;
	known_by: string[];
	timestamp: string;
}

export interface ConversationsDebug {
	exchange_count: number;
	exchanges: ConversationExchangeDebug[];
}

export interface ConversationExchangeDebug {
	timestamp: string;
	speaker_id: number;
	speaker_name: string;
	location_name: string;
	player_input: string;
	npc_dialogue: string;
}

export interface DebugEvent {
	timestamp: string;
	category: string;
	message: string;
}

export interface InferenceCategoryDebug {
	/** Lowercase role name: "dialogue" | "simulation" | "intent" | "reaction". */
	role: string;
	/** Provider override; null means inherit base. */
	provider: string | null;
	/** Model override; null means inherit base model. */
	model: string | null;
	/** Base URL override; null means inherit base. */
	base_url: string | null;
	thinking_level: 'minimal' | 'low' | 'medium' | 'high';
	max_output_tokens: number;
	service_tier: 'standard' | 'priority';
}

export interface InferenceDebug {
	provider_name: string;
	model_name: string;
	base_url: string;
	cloud_provider: string | null;
	cloud_model: string | null;
	has_queue: boolean;
	reaction_req_id: number;
	improv_enabled: boolean;
	call_log: InferenceLogEntry[];
	/** Per-workload provider/model/url and effective inference profile. */
	categories: InferenceCategoryDebug[];
	/** List of provider display names that have an API key configured (or are local). */
	configured_providers: string[];
	/** Cumulative count of Tier 2 JSON parse failures since process start (#29). */
	tier2_parse_failures_total: number;
}

export interface InferenceLogEntry {
	request_id: number;
	timestamp: string;
	model: string;
	provider: string;
	api_mode: string;
	role: 'dialogue' | 'simulation' | 'intent' | 'reaction';
	subrole:
		| 'dialogue'
		| 'intent'
		| 'arrival-reaction'
		| 'message-reaction'
		| 'travel-encounter'
		| 'tier2-simulation'
		| 'tier3-simulation'
		| 'demo-player';
	streaming: boolean;
	duration_ms: number;
	prompt_len: number;
	response_len: number;
	error: string | null;
	system_prompt: string | null;
	prompt_text: string;
	response_text: string;
	max_tokens: number | null;
	ttft_ms: number | null;
	output_tokens: number | null;
	stream_chunks: number | null;
	input_tokens: number | null;
	cached_tokens: number | null;
	thought_tokens: number | null;
	total_tokens: number | null;
	thinking_level: 'minimal' | 'low' | 'medium' | 'high' | null;
	requested_service_tier: 'standard' | 'priority' | null;
	effective_service_tier: string | null;
	provider_request_id: string | null;
	terminal_status: string | null;
	retry_count: number;
	http_status: number | null;
	failure_kind: string | null;
	partial_output_len: number;
	tier_downgraded: boolean;
	estimated_cost_usd: number | null;
	prompt_prefix_hash: string | null;
	prompt_prefix_len: number | null;
}

// ── Persistence types ───────────────────────────────────────────────────────

export interface SnapshotCell {
	id: number;
	game_date: string;
	location: string | null;
}

export interface SaveBranchDisplay {
	name: string;
	id: number;
	parent_name: string | null;
	snapshot_count: number;
	latest_location: string | null;
	latest_game_date: string | null;
	snapshots: SnapshotCell[];
}

export interface SaveFileInfo {
	path: string;
	filename: string;
	file_size: string;
	branches: SaveBranchDisplay[];
	locked: boolean;
}

export interface SaveState {
	filename: string | null;
	branch_id: number | null;
	branch_name: string | null;
}

// ── Demo / auto-player types ─────────────────────────────────────────────────

export interface DemoNpcInfo {
	name: string;
	occupation?: string | null;
	mood: string;
}

export interface DemoAdjacentLocation {
	name: string;
	travel_minutes: number | null;
	visited: boolean;
}

export interface DemoContextSnapshot {
	location_name: string;
	location_description: string;
	game_time: string;
	season: string;
	weather: string;
	npcs_here: DemoNpcInfo[];
	adjacent: DemoAdjacentLocation[];
	active_tasks: PlayerTaskSnapshot[];
	recent_log: string[];
	recent_actions: string[];
	extra_prompt: string | null;
}

export interface DemoConfigPayload {
	auto_start: boolean;
	extra_prompt: string | null;
	turn_pause_secs: number;
	max_turns: number | null;
}

// ── Bug reporting ─────────────────────────────────────────────────────────────

/** A specific debug-panel record attached to a bug report for extra context. */
export interface BugContext {
	/** Record family: "inference", "event", "conversation", etc. */
	kind: string;
	/** Short human-readable label for the record. */
	label: string;
	/** The serialized record itself. */
	detail: unknown;
}

/** A bug report submitted from the toolbar button, a debug record, or MCP. */
export interface BugReportRequest {
	title: string;
	description: string;
	/** `data:image/png;base64,…` screenshot captured by the frontend. */
	screenshot_data_url?: string;
	/** Optional debug-panel record this report was filed from. */
	context?: BugContext;
}

/** Outcome of a bug-report submission. */
export interface BugReportResult {
	/** Whether a GitHub issue was actually created (false in dry-run/offline). */
	created: boolean;
	issue_url?: string;
	issue_number?: number;
	screenshot_url?: string;
	/** On-disk path of the composed bundle, when written (dry-run/offline). */
	bundle_path?: string;
	/** Human-readable summary suitable for a toast. */
	message: string;
}
