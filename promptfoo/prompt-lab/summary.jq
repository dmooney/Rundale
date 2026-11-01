# Summarise one promptfoo results file: jq -f summary.jq output/<round>.json
#
# Each row is one reply from one model to one case under one prompt:
#   scored    both judges answered; the row's score is their mean
#   malformed the model answered, but not with the output schema's JSON (the
#             config's transform marks it "MALFORMED: " and the "schema"
#             assertion scores it 0); scores 0, as the model's miss (§6.4)
#   outage    the request failed (HTTP error, quota) or a judge failed;
#             left out of the score, and the round is rerun (see the skill)
#
# Prices are USD per 1M tokens (input, output), list prices checked
# 2026-11-01. promptfoo's own cost fields came back empty for the judges in
# testing, so spend is computed here from token counts. gemini-3.8-flash is at its introductory price, which
# ends on 2026-12-31 ($1.50 / $7.50 from 2027).
def price: {
  "flash-lite": [0.30, 2.50],
  "flash": [0.75, 3.75],
  "opus": [4.00, 20.00],
  "gpt": [2.00, 10.00]
};
# Gemini reports thinking apart from its output tokens; the Anthropic and
# OpenAI judges already include reasoning in theirs, so it is added for
# Gemini only.
def gemini($who): $who == "flash-lite" or $who == "flash";
# A model with no price counts $0 here and is listed under unpriced_models.
def usd($who; $t):
  (price[$who] // [0, 0]) as $p
  | ((($t.prompt // 0) * $p[0])
     + ((($t.completion // 0) + (if gemini($who) then ($t.completionDetails.reasoning // 0) else 0 end)) * $p[1])) / 1000000;
# The provider label, or its id when it has none (as with -r on the command line).
def model: (.provider.label | select(. != null and . != "")) // .provider.id;
def judges: [.gradingResult.componentResults[]? | select(.assertion.metric == "opus" or .assertion.metric == "gpt") | {metric: .assertion.metric, score, reason, failed: (.metadata.graderError // false), tokens: .tokensUsed}];
def kind:
  if (.response.error != null) or (.response.output == null) then "outage"
  elif .gradingResult == null then "outage"
  elif ([.gradingResult.componentResults[]? | select(.assertion.metric == "schema") | .score] | first) == 0 then "malformed"
  elif (judges | map(select(.failed)) | length) > 0 or (judges | length) < 2 then "outage"
  else "scored" end;
def points: if kind == "scored" then (judges | map(.score) | add / length * 100) elif kind == "malformed" then 0 else null end;
def mean: if length == 0 then null else add / length | . * 10 | round / 10 end;
def summarise:
  {
    replies: length,
    scored: map(select(kind == "scored")) | length,
    malformed: map(select(kind == "malformed")) | length,
    outages: [.[] | select(kind == "outage") | {case: .testCase.metadata.id, model: model, why: ((.response.error // .error // ([judges[] | select(.failed) | .reason] | join("; "))) | tostring | .[0:160])}],
    mean: [.[] | points | select(. != null)] | mean,
    by_requirement: ([.[] | points as $s | select($s != null) | (.testCase.metadata.tags | split(","))[] | {tag: ., s: $s}]
      | group_by(.tag) | map({key: .[0].tag, value: ([.[].s] | mean)}) | from_entries),
    judges_split_over_20: [.[] | select(kind == "scored") | judges as $j | select(((($j[0].score - $j[1].score) | fabs) * 100 | round) > 20)
      | {case: .testCase.metadata.id, model: model, scores: ([$j[] | {(.metric): .score}] | add)}],
    spend_usd: (([.[] | usd(model; .response.tokenUsage // {})] | add // 0)
      + ([.[] | judges[] | usd(.metric; .tokens // {})] | add // 0) | . * 10000 | round / 10000),
    worst: [map(select(points != null)) | sort_by(points)[:6][] | {case: .testCase.metadata.id, model: model, points: points, reply: .response.output, reasons: [judges[] | {(.metric): .reason}]}]
  };

.results.results as $results
| $results
| group_by(.prompt.label)
| map({
    prompt: .[0].prompt.label,
    all_models: summarise,
    by_model: (group_by(model) | map({key: (.[0] | model), value: (summarise | {mean, malformed, outages: (.outages | length), spend_usd})}) | from_entries)
  })
| {prompts: ., round_spend_usd: (map(.all_models.spend_usd) | add)}
| .unpriced_models = ([$results[] | model, (.gradingResult.componentResults[]? | .assertion.metric | select(. != "schema"))] | unique | map(select(price[.] == null)))
