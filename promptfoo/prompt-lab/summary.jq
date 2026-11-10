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
# The models under test run through OpenRouter, which reports what it
# charged for each reply; promptfoo keeps that in the response metadata, and
# spend uses it as billed. (promptfoo's own cost field reads 0 for a provider
# that reports nothing, so it cannot tell a free reply from an unbilled one.)
# A reply with no charge is listed under unpriced_models and counts $0.
# The judges' grader calls carry no charge, so their spend is computed from
# token counts at list prices: USD per 1M tokens (input, output), checked
# 2026-11-01. Both judges count reasoning inside their output tokens.
def price: {
  "opus": [4.00, 20.00],
  "gpt": [2.00, 10.00]
};
def usd($who; $t):
  (price[$who] // [0, 0]) as $p
  | ((($t.prompt // 0) * $p[0]) + (($t.completion // 0) * $p[1])) / 1000000;
def charge: .response.metadata.openrouter.accountCharge?;
def billed: (charge | type) == "number";
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
    spend_usd: (([.[] | select(billed) | charge] | add // 0)
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
| .unpriced_models = (([$results[] | select(.response.output != null and (billed | not)) | model]
    + [$results[] | .gradingResult.componentResults[]? | .assertion.metric | select(. != "schema" and price[.] == null)]) | unique)
