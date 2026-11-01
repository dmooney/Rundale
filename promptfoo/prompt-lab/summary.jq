# Summarise one promptfoo results file: jq -f summary.jq output/<round>.json
# Prices are USD per 1M tokens, list prices checked 2026-11-01. promptfoo does
# not price these judges, so spend is computed here from token counts.
def price: {
  "flash-lite": [0.30, 2.50],
  "opus": [4.00, 20.00],
  "gpt": [2.00, 10.00]
};
def usd($who; $t): (($t.prompt // 0) * price[$who][0] + ($t.completion // 0) * price[$who][1]) / 1000000;
def judges: [.gradingResult.componentResults[]? | {metric: .assertion.metric, score, reason, tokens: .tokensUsed}];

.results.results
| group_by(.prompt.label)
| map({
    prompt: .[0].prompt.label,
    replies: length,
    errors: [.[] | select(.error and (.gradingResult == null))] | length,
    mean: (([.[] | select(.gradingResult != null) | .score] | add / length * 100) // null),
    by_requirement: (
      [.[] | select(.gradingResult != null) | .score as $s | (.testCase.metadata.tags | split(","))[] | {tag: ., s: $s}]
      | group_by(.tag) | map({key: .[0].tag, value: ([.[].s] | add / length * 100 | round)}) | from_entries),
    judges_split_over_20: [.[] | judges as $j | select(($j | length) == 2 and ((($j[0].score // 0) - ($j[1].score // 0)) | fabs) > 0.2)
      | {case: .testCase.metadata.id, scores: [$j[] | {(.metric): .score}] | add}],
    spend_usd: (
      ([.[] | usd("flash-lite"; .response.tokenUsage // {})] | add)
      + ([.[] | judges[] | usd(.metric; .tokens // {})] | add // 0)
      | . * 10000 | round / 10000),
    worst: [sort_by(.score)[:6][] | {case: .testCase.metadata.id, score: (.score * 100 | round), reply: .response.output, reasons: [judges[] | {(.metric): .reason}]}]
  })
