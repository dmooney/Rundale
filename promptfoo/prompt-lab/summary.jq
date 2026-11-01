# Summarise one promptfoo results file: jq -f summary.jq output/<round>.json
#
# Each row is one reply from one model to one case under one prompt:
#   scored    both judges answered; the row's score is their mean
#   malformed the model answered, but not with the output schema's JSON;
#             scores 0, as the model's miss (§6.4: "all training replies")
#   outage    the request failed (HTTP error, quota) or a judge failed;
#             left out of the score, and the round is rerun (see the skill)
#
# Prices are USD per 1M tokens (input, output), list prices checked
# 2026-11-01. promptfoo does not price these judges, so spend is computed here
# from token counts. gemini-3.8-flash is at its introductory price, which
# ends on 2026-12-31 ($1.50 / $7.50 from 2027).
def price: {
  "flash-lite": [0.30, 2.50],
  "flash": [0.75, 3.75],
  "opus": [4.00, 20.00],
  "gpt": [2.00, 10.00]
};
def usd($who; $t):
  ((($t.prompt // 0) * price[$who][0])
   + ((($t.completion // 0) + ($t.completionDetails.reasoning // 0)) * price[$who][1])) / 1000000;
def judges: [.gradingResult.componentResults[]? | {metric: .assertion.metric, score, reason, failed: (.metadata.graderError // false), tokens: .tokensUsed}];
def kind:
  if (.response.error != null) or (.response.output == null) then "outage"
  elif .gradingResult == null then "malformed"
  elif (judges | map(select(.failed)) | length) > 0 or (judges | length) < 2 then "outage"
  else "scored" end;
def points: if kind == "scored" then .score * 100 elif kind == "malformed" then 0 else null end;
def mean: if length == 0 then null else add / length | . * 10 | round / 10 end;
def summarise:
  {
    replies: length,
    scored: map(select(kind == "scored")) | length,
    malformed: map(select(kind == "malformed")) | length,
    outages: [.[] | select(kind == "outage") | {case: .testCase.metadata.id, model: .provider.label, why: ((.response.error // .error // ([judges[] | select(.failed) | .reason] | join("; "))) | tostring | .[0:160])}],
    mean: [.[] | points | select(. != null)] | mean,
    by_requirement: ([.[] | points as $s | select($s != null) | (.testCase.metadata.tags | split(","))[] | {tag: ., s: $s}]
      | group_by(.tag) | map({key: .[0].tag, value: ([.[].s] | mean)}) | from_entries),
    judges_split_over_20: [.[] | select(kind == "scored") | judges as $j | select(((($j[0].score - $j[1].score) | fabs) * 100 | round) > 20)
      | {case: .testCase.metadata.id, model: .provider.label, scores: ([$j[] | {(.metric): .score}] | add)}],
    spend_usd: (([.[] | usd(.provider.label; .response.tokenUsage // {})] | add // 0)
      + ([.[] | judges[] | usd(.metric; .tokens // {})] | add // 0) | . * 10000 | round / 10000),
    worst: [map(select(points != null)) | sort_by(points)[:6][] | {case: .testCase.metadata.id, model: .provider.label, points: points, reply: .response.output, reasons: [judges[] | {(.metric): .reason}]}]
  };

.results.results
| group_by(.prompt.label)
| map({
    prompt: .[0].prompt.label,
    all_models: summarise,
    by_model: (group_by(.provider.label) | map({key: .[0].provider.label, value: (summarise | {mean, malformed, outages: (.outages | length), spend_usd})}) | from_entries)
  })
| {prompts: ., round_spend_usd: (map(.all_models.spend_usd) | add)}
