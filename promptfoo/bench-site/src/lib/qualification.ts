import fs from 'node:fs';
import path from 'node:path';

export interface QualificationRun {
	run_id: string;
	tested_on: string;
	candidate: string;
	model: string;
	status: 'invalid_profile' | 'rejected' | 'stopped' | 'needs_performance' | 'needs_judgment' | 'needs_adjudication' | 'quality_rejected' | 'qualified';
	stage: string;
	reason: string;
	preflight: {
		calls: number;
		valid: number;
		guard_interventions: number;
		guard_rate: number | null;
		elapsed_min_ms?: number | null;
		elapsed_max_ms?: number | null;
		artifact: { path: string; sha256: string };
		request_profile?: {
			enable_thinking?: boolean | null;
			reasoning_effort?: string | null;
			frequency_penalty?: number | null;
			json_mode?: boolean | null;
			max_tokens?: number | null;
			model?: string | null;
			temperature?: number | null;
		};
	};
	performance: null | {
		measurements: number;
		cold_measurements: number;
		warm_measurements: number;
		cold_ttft_p95_ms: number | null;
		warm_ttft_p95_ms: number | null;
		cold_completion_p95_ms: number | null;
		warm_completion_p95_ms: number | null;
		tokens_per_second_p50: number | null;
		error_rate: number;
		speed_index_ms?: number;
		speed_rank?: number;
		speed_cohort_size?: number;
		artifact: { path: string; sha256: string };
	};
	judgment: null | {
		method: string;
		candidate_family: string;
		overall: number | null;
		axes: Record<string, number>;
		hard_failures: Record<string, number>;
		pass: boolean;
		complete: boolean;
		needs_adjudication: boolean;
		overall_spread: number | null;
		cost_usd: number;
		votes: { eligible: number; required: number; pass: number; fail: number; self_excluded: number };
		sample: { items: number; judged_items: number; unusable_outputs: number };
		judges: Array<{
			id: string;
			family: string;
			eligible: boolean;
			exclusion_reason: string | null;
			overall: number;
			pass: boolean;
			artifact: { path: string; sha256: string };
			judge: { model: string; provider: string; reasoning_effort: string; cost_usd: number | null };
		}>;
		quality_rank?: number;
		quality_cohort_size?: number;
	};
	calls: {
		count: number;
		preflight: number;
		diagnostic: number;
		performance: number;
		judgment: number;
		path: string;
		sha256: string;
	};
}

export interface QualificationPolicy {
	version: number;
	preflight: {
		calls: number;
		minimum_valid_response_rate: number;
	};
	guards: {
		maximum_intervention_rate: number;
	};
	performance: {
		minimum_measurements: number;
		minimum_cold_measurements: number;
		minimum_warm_measurements: number;
		maximum_error_rate: number;
		ranking: {
			warm_ttft_weight: number;
			warm_completion_weight: number;
			lower_is_better: boolean;
		};
	};
	judgment: {
		minimum_overall: number;
		minimum_critical_axis: number;
		critical_axes: string[];
		maximum_hard_failures: number;
		minimum_independent_judges: number;
		exclude_same_family: boolean;
		consensus_method: string;
		maximum_overall_spread_without_adjudication: number;
		tie_margin: number;
		judges: Array<{
			id: string;
			model: string;
			family: string;
			provider: string;
			reasoning_effort: string;
			max_tokens: number;
		}>;
	};
}

export interface QualificationData {
	version: number;
	source: string;
	counts: Record<string, number>;
	policy?: QualificationPolicy;
	runs: QualificationRun[];
}

export function loadQualificationData(): QualificationData {
	const root = process.env.RB_PROMPTFOO_DIR
		? path.resolve(process.env.RB_PROMPTFOO_DIR)
		: path.resolve(process.cwd(), '..');
	const file = path.join(root, 'leaderboard', 'dialogue-qualification.json');
	if (!fs.existsSync(file)) return { version: 1, source: '', counts: {}, runs: [] };
	return JSON.parse(fs.readFileSync(file, 'utf-8')) as QualificationData;
}
