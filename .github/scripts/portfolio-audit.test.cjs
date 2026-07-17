"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const {
  auditIssue,
  auditPortfolio,
  auditRoadmap,
  hasUnblockTrigger,
} = require("./portfolio-audit.cjs");

const vocabulary = {
  priorities: ["P0", "P1", "P2", "P3"],
  themes: ["bug", "enhancement", "infra", "refactor"],
};

function activeIssue(overrides = {}) {
  return {
    number: 1,
    title: "Managed work",
    labels: ["P2", "enhancement", "in-progress", "codex-automation"],
    assignees: [],
    openClosingPrNumbers: [10],
    body: [
      "## Outcome",
      "A measurable result.",
      "- Initiative: Governance",
      "- DRI: Codex",
      "## Acceptance criteria",
      "- [ ] Result is verified.",
      "## Proof method",
      "Run the focused tests.",
    ].join("\n\n"),
    ...overrides,
  };
}

function issueTriggerEventTypes(workflow) {
  const issueTrigger = workflow.match(
    /^ {2}issues:[ \t]*\r?\n {4,}types:[ \t]*\[([^\]\r\n]+)\][ \t]*\r?$/m,
  );

  assert.ok(issueTrigger, "triage audit must retain its issues trigger");
  return issueTrigger[1]
    .split(",")
    .map((event) => event.trim().replace(/^(['"])(.*)\1$/, "$2"));
}

test("triage audit runs for newly opened issues", () => {
  const workflowPath = path.join(__dirname, "..", "workflows", "triage-audit.yml");
  const workflow = fs.readFileSync(workflowPath, "utf8");
  const eventTypes = issueTriggerEventTypes(workflow);
  assert.ok(eventTypes.includes("opened"), "issues trigger must include opened");
});
test("triage audit parser accepts CRLF, spacing, and quoted inline event types", () => {
  const workflow = [
    "on:",
    "  issues:   ",
    "      types: [\"opened\", 'edited']",
    "  pull_request:",
    "    types: [opened]",
    "",
  ].join("\r\n");

  assert.deepEqual(issueTriggerEventTypes(workflow), ["opened", "edited"]);
});

test("triage audit sensor does not borrow opened from pull_request", () => {
  const workflow = [
    "on:",
    "  issues:",
    "    types: [edited, closed]",
    "  pull_request:",
    "    types: [opened]",
    "",
  ].join("\n");

  assert.equal(issueTriggerEventTypes(workflow).includes("opened"), false);
});

test("a complete active issue passes the portfolio audit", () => {
  assert.deepEqual(auditIssue(activeIssue(), vocabulary).problems, []);
});

test("epics receive readiness checks without consuming work-unit WIP or requiring one PR", () => {
  const epic = activeIssue({
    labels: ["P2", "enhancement", "in-progress", "epic"],
    openClosingPrNumbers: [],
  });
  const finding = auditIssue(epic, vocabulary);

  assert.equal(finding.active, true);
  assert.equal(finding.workUnitActive, false);
  assert.ok(!finding.problems.includes("authoritative open closing PR"));
  assert.equal(
    auditPortfolio({ issues: [epic], pullRequests: [], vocabulary }).wip.implementing,
    0,
  );
});

test("active work reports missing readiness fields and authoritative PR linkage", () => {
  const finding = auditIssue(
    activeIssue({ body: "No structured readiness data.", openClosingPrNumbers: [] }),
    vocabulary,
  );

  assert.deepEqual(finding.problems, [
    "Outcome/Objective",
    "Initiative",
    "Acceptance/Completion criteria",
    "Proof method",
    "DRI",
    "authoritative open closing PR",
  ]);
});

test("blocked and deferred work requires a substantive unblock trigger", () => {
  const blocked = activeIssue({
    labels: ["P2", "enhancement"],
    body: "- Status: Blocked\n\n## Outcome\nWaiting.",
  });
  assert.ok(auditIssue(blocked, vocabulary).problems.includes("Unblock trigger"));
  assert.equal(hasUnblockTrigger("## Unblock trigger\n\nA live MLX runtime is available."), true);
  assert.equal(hasUnblockTrigger("## Unblock trigger\n\nNot applicable"), false);
  assert.equal(
    hasUnblockTrigger(
      "## Unblock trigger\n\nNot applicable.\nA funded runtime becomes available.\n\n## Proof\nRun it.",
    ),
    true,
  );
  assert.equal(
    hasUnblockTrigger("## Unblock trigger\n\nNot applicable.\n\n## Proof\nRun it."),
    false,
  );
});

test("incidental issue numbers never count as authoritative PR linkage", () => {
  const result = auditPortfolio({
    issues: [activeIssue({ openClosingPrNumbers: [] })],
    pullRequests: [
      {
        number: 10,
        title: "Dependency update mentioning #1 in its changelog",
        body: "Release notes: fixed upstream #1",
        closingIssueNumbers: [],
        draft: false,
      },
    ],
    vocabulary,
  });

  assert.ok(
    result.issueFindings[0].problems.includes("authoritative open closing PR"),
  );
  assert.deepEqual(result.unlinkedPullRequests, [
    { number: 10, title: "Dependency update mentioning #1 in its changelog" },
  ]);
});

test("automated dependency PRs are exempt from work-item issue linkage", () => {
  const result = auditPortfolio({
    issues: [],
    pullRequests: [
      {
        number: 20,
        title: "Bump a dependency",
        closingIssueNumbers: [],
        draft: false,
        user: { login: "dependabot[bot]" },
      },
    ],
    vocabulary,
  });

  assert.deepEqual(result.unlinkedPullRequests, []);
});

test("implementation and review buffers report independent backpressure", () => {
  const issues = [1, 2, 3, 4].map((number) => activeIssue({ number }));
  const pullRequests = [1, 2, 3, 4].map((number) => ({
    number,
    title: `PR ${number}`,
    closingIssueNumbers: [number],
    draft: false,
  }));
  const result = auditPortfolio({ issues, pullRequests, vocabulary });

  assert.equal(result.wip.implementationExceeded, true);
  assert.equal(result.wip.reviewExceeded, true);
  assert.equal(result.wip.implementing, 4);
  assert.equal(result.wip.ciReview, 4);
});

test("roadmap audit compares explicit epic state mappings", () => {
  const entries = [
    {
      issue: 1626,
      row: "Illustrated notebook play surface",
      openStatus: "In progress",
      closedStatus: "Implemented",
    },
  ];
  const roadmap = [
    "| Subsystem / Feature | Status | Design |",
    "| --- | --- | --- |",
    "| Illustrated notebook play surface | In progress | plan |",
  ].join("\n");

  assert.deepEqual(auditRoadmap(entries, { 1626: "OPEN" }, roadmap), []);
  assert.deepEqual(auditRoadmap(entries, { 1626: "CLOSED" }, roadmap), [
    {
      ...entries[0],
      expected: "Implemented",
      actual: "In progress",
      problem: "roadmap status mismatch",
    },
  ]);
});
