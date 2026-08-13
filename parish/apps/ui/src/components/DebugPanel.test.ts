import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import {
	debugVisible,
	debugSnapshot,
	debugTab,
	debugDockLeft,
	selectedNpcId,
} from '../stores/debug';
import type { DebugSnapshot } from '$lib/types';
import DebugPanel from './DebugPanel.svelte';

// The Inference tab imports submitInput at module scope.
vi.mock('$lib/ipc', () => ({
	submitInput: vi.fn(() => Promise.resolve()),
}));

// ── Snapshot factory ─────────────────────────────────────────────────────────

function makeSnapshot(overrides: Partial<DebugSnapshot> = {}): DebugSnapshot {
	return {
		clock: {
			game_time: '1820-03-15 09:00',
			time_of_day: 'Morning',
			season: 'Spring',
			festival: null,
			weather: 'Clear',
			paused: false,
			inference_paused: false,
			speed_factor: 1.0,
			speed_name: null,
			day_of_week: 'Wednesday',
			day_type: 'Workday',
			start_game_time: '1820-03-15 06:00',
			paused_game_time: '1820-03-15 09:00',
			real_elapsed_secs: 0,
		},
		weather: {
			current: 'Clear',
			since: '1820-03-15 06:00',
			duration_hours: 3.0,
			min_duration_hours: 2.0,
			last_check_at: '08:00 1820-03-15',
		},
		world: {
			player_location_name: 'Village Green',
			player_location_id: 1,
			location_count: 5,
			visited_count: 2,
			visited_locations: ['Village Green', 'Pub'],
			edge_traversals: [],
			text_log_tail: [],
			text_log_len: 0,
			locations: [],
			player_name: null,
		},
		npcs: [],
		tier_summary: {
			tier1_count: 1,
			tier2_count: 2,
			tier3_count: 3,
			tier4_count: 0,
			tier1_names: ['Brigid'],
			tier2_names: ['Seamus', 'Nora'],
			tier3_names: [],
			tier4_names: [],
			tier3_in_flight: false,
			last_tier2_tick: null,
			last_tier3_tick: null,
			last_tier4_tick: null,
			introduced_count: 2,
			tier2_in_flight: false,
			tier3_pending_count: 0,
			tier4_recent_events: [],
		},
		event_bus: {
			subscriber_count: 3,
			recent_events: [],
		},
		gossip: {
			item_count: 0,
			items: [],
		},
		conversations: {
			exchange_count: 0,
			exchanges: [],
		},
		events: [],
		inference: {
			provider_name: 'anthropic',
			model_name: 'claude-3-haiku',
			base_url: '',
			cloud_provider: null,
			cloud_model: null,
			has_queue: false,
			reaction_req_id: 0,
			improv_enabled: false,
			call_log: [],
			categories: [],
			configured_providers: [],
			tier2_parse_failures_total: 0,
		},
		auth: {
			oauth_enabled: false,
			logged_in: false,
			provider: null,
			display_name: null,
			session_id: null,
		},
		...overrides,
	};
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function resetStores() {
	debugVisible.set(false);
	debugSnapshot.set(null);
	debugTab.set(0);
	debugDockLeft.set(false);
	selectedNpcId.set(null);
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('DebugPanel', () => {
	beforeEach(resetStores);

	it('renders nothing when debugVisible is false', () => {
		debugSnapshot.set(makeSnapshot());
		debugVisible.set(false);
		const { container } = render(DebugPanel);
		expect(container.querySelector('.debug-panel')).toBeFalsy();
	});

	it('renders nothing when snapshot is null even if visible', () => {
		debugVisible.set(true);
		debugSnapshot.set(null);
		const { container } = render(DebugPanel);
		expect(container.querySelector('.debug-panel')).toBeFalsy();
	});

	describe('Overview tab (index 0)', () => {
		it('renders game clock and tier summary', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(0);
			const { getByText } = render(DebugPanel);

			// Game clock
			expect(getByText('1820-03-15 09:00')).toBeTruthy();

			// Tier counts (rendered as "T1: 1 | T2: 2 | T3: 3 | T4: 0")
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('T1: 1');
			expect(container.textContent).toContain('T2: 2');
		});

		it('shows tier names when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(0);
			const { getByText } = render(DebugPanel);
			expect(getByText(/Brigid/)).toBeTruthy();
		});
	});

	describe('NPCs tab (index 1)', () => {
		const npcFixture = {
			id: 42,
			name: 'Máire Ní Bhriain',
			brief_description: 'A local weaver.',
			introduced: true,
			age: 35,
			occupation: 'Weaver',
			personality: 'Quiet and thoughtful.',
			location_name: 'Village Green',
			location_id: 1,
			home_name: 'Cottage',
			workplace_name: null,
			mood: 'content',
			is_ill: false,
			state: 'Present',
			tier: 'T1',
			schedule: [],
			relationships: [],
			memories: [],
			long_term_memories: [],
			reactions: [],
			deflated_summary: null,
			knowledge: [],
			intelligence: {
				verbal: 4,
				analytical: 3,
				emotional: 5,
				practical: 3,
				wisdom: 4,
				creative: 2,
			},
			last_activity: null,
			knows_player_name: false,
		};

		it('shows NPC list when no NPC is selected', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot({ npcs: [npcFixture] }));
			debugTab.set(1);
			const { getByText } = render(DebugPanel);
			expect(getByText('Máire Ní Bhriain')).toBeTruthy();
		});

		it('clicking an NPC row shows the detail view', async () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot({ npcs: [npcFixture] }));
			debugTab.set(1);
			const { getByText, container } = render(DebugPanel);

			const npcBtn = container.querySelector('.npc-row') as HTMLButtonElement;
			expect(npcBtn).toBeTruthy();
			await fireEvent.click(npcBtn);
			await tick();

			// Detail view shows back button and NPC name as heading
			expect(getByText('Back to list')).toBeTruthy();
			// Detail shows identity info
			expect(container.textContent).toContain('Weaver');
			expect(container.textContent).toContain('35');
		});

		it('shows (no NPCs) when list is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot({ npcs: [] }));
			debugTab.set(1);
			const { getByText } = render(DebugPanel);
			expect(getByText('(no NPCs)')).toBeTruthy();
		});
	});

	describe('World tab (index 2)', () => {
		it('renders without throwing and shows location counts', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(2);
			const { container } = render(DebugPanel);
			// Shows visited/total counts
			expect(container.textContent).toContain('2/5');
		});

		it('shows "(empty)" when text log is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(2);
			const { getByText } = render(DebugPanel);
			expect(getByText('(empty)')).toBeTruthy();
		});
	});

	describe('Weather tab (index 3)', () => {
		it('renders weather engine details', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(3);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('Weather Engine');
			expect(container.textContent).toContain('Clear');
			expect(container.textContent).toContain('1820-03-15 06:00');
			expect(container.textContent).toContain('3.00h');
			expect(container.textContent).toContain('Last checked');
			expect(container.textContent).toContain('08:00 1820-03-15');
		});

		it('shows "(never)" when last_check_at is null', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					weather: {
						current: 'Rain',
						since: '1820-03-15 08:00',
						duration_hours: 1.0,
						min_duration_hours: 0.5,
						last_check_at: null,
					},
				}),
			);
			debugTab.set(3);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('(never)');
		});
	});

	describe('Gossip tab (index 4)', () => {
		it('shows "(no gossip)" when gossip list is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(4);
			const { getByText } = render(DebugPanel);
			expect(getByText('(no gossip)')).toBeTruthy();
		});

		it('renders gossip items when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					gossip: {
						item_count: 2,
						items: [
							{
								id: 1,
								content: 'The harvest looks promising.',
								source_name: 'Brigid',
								known_by: ['Brigid', 'Seamus'],
								distortion_level: 0,
								timestamp: '09:00',
							},
							{
								id: 2,
								content: 'Rent collector was seen on the road.',
								source_name: 'Padraig',
								known_by: ['Padraig'],
								distortion_level: 2,
								timestamp: '09:30',
							},
						],
					},
				}),
			);
			debugTab.set(4);
			const { container, getByText } = render(DebugPanel);
			expect(getByText(/The harvest looks promising/)).toBeTruthy();
			expect(getByText(/Rent collector was seen/)).toBeTruthy();
			expect(container.textContent).toContain('[distortion 2]');
			expect(container.textContent).toContain('Brigid');
		});
	});

	describe('Conversations tab (index 5)', () => {
		it('shows "(no exchanges)" when conversation log is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(5);
			const { getByText } = render(DebugPanel);
			expect(getByText('(no exchanges)')).toBeTruthy();
		});

		it('renders conversation exchanges when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					conversations: {
						exchange_count: 1,
						exchanges: [
							{
								timestamp: '09:15',
								speaker_id: 1,
								location_name: 'Village Green',
								player_input: 'Good day to you.',
								speaker_name: 'Brigid',
								npc_dialogue: 'And a good day to yourself, stranger.',
							},
						],
					},
				}),
			);
			debugTab.set(5);
			const { getByText } = render(DebugPanel);
			expect(getByText(/Good day to you/)).toBeTruthy();
			expect(getByText(/Brigid/)).toBeTruthy();
			expect(getByText(/good day to yourself/)).toBeTruthy();
		});
	});

	describe('Events tab (index 6)', () => {
		it('shows "(no game events captured)" when event bus is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(6);
			const { getByText } = render(DebugPanel);
			expect(getByText('(no game events captured)')).toBeTruthy();
		});

		it('renders game events from event_bus when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					event_bus: {
						subscriber_count: 2,
						recent_events: [
							{
								timestamp: '09:01',
								kind: 'NpcMoved',
								summary: 'Seamus walked to the pub.',
							},
						],
					},
				}),
			);
			debugTab.set(6);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('NpcMoved');
			expect(container.textContent).toContain('Seamus walked to the pub.');
		});

		it('renders debug events when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					events: [
						{
							timestamp: '09:02',
							category: 'system',
							message: 'World ticked.',
						},
					],
				}),
			);
			debugTab.set(6);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('system');
			expect(container.textContent).toContain('World ticked.');
		});
	});

	describe('Inference tab (index 7)', () => {
		it('shows native Gemini role profiles and cache performance', () => {
			debugVisible.set(true);
			const snap = makeSnapshot();
			snap.inference.categories = [
				{
					role: 'dialogue',
					provider: 'google',
					model: 'gemini-3.6-flash',
					base_url: 'https://generativelanguage.googleapis.com/v1',
					thinking_level: 'medium',
					max_output_tokens: 4096,
					service_tier: 'standard',
				},
			];
			snap.inference.call_log = [
				{
					request_id: 36,
					timestamp: '09:36',
					model: 'gemini-3.6-flash',
					provider: 'google',
					api_mode: 'google-interactions-v1',
					role: 'dialogue',
					subrole: 'dialogue',
					streaming: true,
					duration_ms: 420,
					prompt_len: 9000,
					response_len: 40,
					error: null,
					system_prompt: null,
					prompt_text: '',
					response_text: '',
					max_tokens: 4096,
					ttft_ms: 120,
					output_tokens: 10,
					stream_chunks: 4,
					input_tokens: 9000,
					cached_tokens: 8000,
					thought_tokens: 20,
					total_tokens: 9030,
					thinking_level: 'medium',
					requested_service_tier: 'standard',
					effective_service_tier: 'standard',
					provider_request_id: 'int_test',
					terminal_status: 'completed',
					retry_count: 0,
					http_status: 200,
					failure_kind: null,
					partial_output_len: 0,
					tier_downgraded: false,
					estimated_cost_usd: 0.002,
					prompt_prefix_hash: 'abcd',
					prompt_prefix_len: 8500,
				},
			];
			debugSnapshot.set(snap);
			debugTab.set(7);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('gemini-3.6-flash');
			expect(container.textContent).toContain('medium');
			expect(container.textContent).toContain('standard');
			expect(container.textContent).toContain('88.9%');
			expect(container.textContent).toContain('Session: 1 calls');
			expect(container.textContent).toContain(
				'input/cached/thought/output/total 9000/8000/20/10/9030',
			);
			expect(container.textContent).toContain('estimated cost $0.00200');
		});

		it('shows call log entries when present', () => {
			debugVisible.set(true);
			debugSnapshot.set(
				makeSnapshot({
					inference: {
						provider_name: 'anthropic',
						model_name: 'claude-3-haiku',
						base_url: '',
						cloud_provider: null,
						cloud_model: null,
						has_queue: false,
						reaction_req_id: 0,
						improv_enabled: false,
						categories: [],
						configured_providers: ['anthropic'],
						tier2_parse_failures_total: 0,
						call_log: [
							{
								request_id: 1,
								timestamp: '09:03',
								model: 'claude-3-haiku',
								provider: 'anthropic',
								api_mode: 'anthropic-messages',
								role: 'dialogue',
								subrole: 'dialogue',
								streaming: false,
								duration_ms: 350,
								prompt_len: 120,
								response_len: 80,
								error: null,
								system_prompt: 'You are Brigid, a local woman.',
								prompt_text: 'What do you think?',
								response_text: 'I think the harvest will be poor this year.',
								max_tokens: null,
								ttft_ms: null,
								output_tokens: null,
								stream_chunks: null,
								input_tokens: null,
								cached_tokens: null,
								thought_tokens: null,
								total_tokens: null,
								thinking_level: null,
								requested_service_tier: null,
								effective_service_tier: null,
								provider_request_id: null,
								terminal_status: 'completed',
								retry_count: 0,
								http_status: 200,
								failure_kind: null,
								partial_output_len: 0,
								tier_downgraded: false,
								estimated_cost_usd: null,
								prompt_prefix_hash: null,
								prompt_prefix_len: null,
							},
						],
					},
				}),
			);
			debugTab.set(7);
			const { container } = render(DebugPanel);
			// Call log should show the entry
			expect(container.textContent).toContain('#1');
			expect(container.textContent).toContain('350ms');
		});

		it('shows "(no calls yet)" when call log is empty', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(7);
			const { getByText } = render(DebugPanel);
			expect(getByText('(no calls yet)')).toBeTruthy();
		});

		it('shows provider name', () => {
			debugVisible.set(true);
			debugSnapshot.set(makeSnapshot());
			debugTab.set(7);
			const { container } = render(DebugPanel);
			expect(container.textContent).toContain('anthropic');
		});

		it('resets selectedLogId when panel is closed so list view shows on reopen (regression #775)', async () => {
			// selectedLogId is a component-level `let` — it is initialised once at construction,
			// not on each visibility toggle, so stale values survive open/close cycles.
			// The fix (closePanel()) must null it via the X button handler.
			const snap = makeSnapshot({
				inference: {
					provider_name: 'anthropic',
					model_name: 'claude-3-haiku',
					base_url: '',
					cloud_provider: null,
					cloud_model: null,
					has_queue: false,
					reaction_req_id: 0,
					improv_enabled: false,
					categories: [],
					configured_providers: ['anthropic'],
					tier2_parse_failures_total: 0,
					call_log: [
						{
							request_id: 5,
							timestamp: '09:10',
							model: 'claude-3-haiku',
							provider: 'anthropic',
							api_mode: 'anthropic-messages',
							role: 'dialogue',
							subrole: 'dialogue',
							streaming: false,
							duration_ms: 200,
							prompt_len: 50,
							response_len: 30,
							error: null,
							system_prompt: null,
							prompt_text: 'Hello',
							response_text: 'Hi there.',
							max_tokens: null,
							ttft_ms: null,
							output_tokens: null,
							stream_chunks: null,
							input_tokens: null,
							cached_tokens: null,
							thought_tokens: null,
							total_tokens: null,
							thinking_level: null,
							requested_service_tier: null,
							effective_service_tier: null,
							provider_request_id: null,
							terminal_status: 'completed',
							retry_count: 0,
							http_status: 200,
							failure_kind: null,
							partial_output_len: 0,
							tier_downgraded: false,
							estimated_cost_usd: null,
							prompt_prefix_hash: null,
							prompt_prefix_len: null,
						},
					],
				},
			});

			debugSnapshot.set(snap);
			debugTab.set(7);
			debugVisible.set(true);

			const { container } = render(DebugPanel);

			// Select the log entry — this sets selectedLogId = 5 inside the component.
			const logRow = container.querySelector('.log-row') as HTMLButtonElement;
			expect(logRow).toBeTruthy();
			await fireEvent.click(logRow);
			await tick();

			// Confirm we are in the detail view.
			expect(container.querySelector('.log-detail-header')).toBeTruthy();

			// Close via the X button so closePanel() runs (not debugVisible.set directly —
			// that would bypass the fix and make the test vacuous).
			const closeBtn = container.querySelector(
				'.debug-close',
			) as HTMLButtonElement;
			expect(closeBtn).toBeTruthy();
			await fireEvent.click(closeBtn);
			await tick();

			// Reopen the panel on the same snapshot (same request_id=5 is still present).
			debugVisible.set(true);
			await tick();

			// After closePanel() the list view should be shown, not the stale detail view.
			expect(container.querySelector('.log-detail-header')).toBeFalsy();
			expect(container.querySelector('.log-row')).toBeTruthy();
		});
	});
});
