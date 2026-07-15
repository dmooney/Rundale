"use strict";

const ACTIVE_LABEL = "in-progress";
const DEFAULT_IMPLEMENTATION_LIMIT = 3;
const DEFAULT_REVIEW_LIMIT = 3;

function labelNames(item) {
  return (item.labels || []).map((label) =>
    typeof label === "string" ? label : label.name,
  );
}

function hasHeading(body, names) {
  const escaped = names.map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(`^#{2,3}\\s+(?:${escaped.join("|")})\\s*$`, "im");
  return pattern.test(body || "");
}

function hasField(body, names) {
  const text = body || "";
  return names.some((name) => {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const bullet = new RegExp(`^-\\s*${escaped}:\\s*\\S.+$`, "im");
    const heading = new RegExp(`^###\\s+${escaped}\\s*$`, "im");
    return bullet.test(text) || heading.test(text);
  });
}

function portfolioStatus(body) {
  const text = body || "";
  const bullet = text.match(/^-\s*(?:Status|Portfolio status):\s*(.+)$/im);
  if (bullet) return bullet[1].trim();

  const form = text.match(/^###\s+Portfolio status\s*\n+\s*([^\n]+)/im);
  return form ? form[1].trim() : "";
}

function hasUnblockTrigger(body) {
  const text = body || "";
  if (hasHeading(text, ["Unblock trigger"])) {
    const section = text.match(
      /(?:^|\r?\n)#{2,3}\s+Unblock trigger\s*\r?\n+([\s\S]*?)(?=(?:\r?\n)#{2,3}\s+|$)/i,
    );
    return Boolean(section && section[1].trim() && !/^not applicable\.?$/i.test(section[1].trim()));
  }
  const bullet = text.match(/^-\s*Unblock Trigger:\s*(.+)$/im);
  return Boolean(bullet && !/^not applicable\.?$/i.test(bullet[1].trim()));
}

function auditIssue(issue, vocabulary) {
  const labels = labelNames(issue);
  const priority = labels.filter((name) => vocabulary.priorities.includes(name));
  const themes = labels.filter((name) => vocabulary.themes.includes(name));
  const active = labels.includes(ACTIVE_LABEL);
  const workUnitActive = active && !labels.includes("epic");
  const status = portfolioStatus(issue.body);
  const problems = [];

  if (priority.length !== 1) problems.push("exactly one P0-P3 severity label");
  if (themes.length === 0) problems.push("at least one canonical theme label");

  if (active) {
    if (!hasHeading(issue.body, ["Outcome", "Objective"])) problems.push("Outcome/Objective");
    if (!hasField(issue.body, ["Initiative"])) problems.push("Initiative");
    if (!hasHeading(issue.body, ["Acceptance criteria", "Completion criteria"])) {
      problems.push("Acceptance/Completion criteria");
    }
    if (!hasHeading(issue.body, ["Proof", "Proof method", "Work-item gates"])) {
      problems.push("Proof method");
    }
    if (!hasField(issue.body, ["DRI"]) && !(issue.assignees || []).length) {
      problems.push("DRI");
    }
  }

  if (/^(blocked|deferred)\b/i.test(status) && !hasUnblockTrigger(issue.body)) {
    problems.push("Unblock trigger");
  }

  if (workUnitActive && !(issue.openClosingPrNumbers || []).length) {
    problems.push("authoritative open closing PR");
  }

  return {
    number: issue.number,
    title: issue.title,
    active,
    workUnitActive,
    status,
    problems,
  };
}

function isAutomatedDependencyPr(pr) {
  return pr.user?.login === "dependabot[bot]";
}

function auditRoadmap(entries, issueStates, roadmapText) {
  const rows = new Map();
  for (const line of (roadmapText || "").split("\n")) {
    if (!line.trimStart().startsWith("|")) continue;
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((cell) => cell.trim());
    if (cells.length >= 2) rows.set(cells[0], cells[1]);
  }

  return entries.flatMap((entry) => {
    const state = issueStates[entry.issue] || issueStates[String(entry.issue)];
    const expected = state === "CLOSED" ? entry.closedStatus : entry.openStatus;
    const actual = rows.get(entry.row);
    if (!state) {
      return [{ ...entry, expected, actual, problem: "epic state unavailable" }];
    }
    if (!actual) {
      return [{ ...entry, expected, actual, problem: "roadmap row missing" }];
    }
    if (actual !== expected) {
      return [{ ...entry, expected, actual, problem: "roadmap status mismatch" }];
    }
    return [];
  });
}

function auditPortfolio({
  issues,
  pullRequests,
  vocabulary,
  implementationLimit = DEFAULT_IMPLEMENTATION_LIMIT,
  reviewLimit = DEFAULT_REVIEW_LIMIT,
}) {
  const issueFindings = issues.map((issue) => auditIssue(issue, vocabulary));
  const activeIssues = issueFindings.filter((finding) => finding.workUnitActive);
  const reviewPullRequests = pullRequests.filter((pr) => !pr.draft);
  const unlinkedPullRequests = pullRequests
    .filter(
      (pr) =>
        !isAutomatedDependencyPr(pr) && !(pr.closingIssueNumbers || []).length,
    )
    .map((pr) => ({ number: pr.number, title: pr.title }));

  return {
    issueFindings,
    unlinkedPullRequests,
    wip: {
      implementing: activeIssues.length,
      implementationLimit,
      implementationExceeded: activeIssues.length > implementationLimit,
      ciReview: reviewPullRequests.length,
      reviewLimit,
      reviewExceeded: reviewPullRequests.length > reviewLimit,
    },
  };
}

module.exports = {
  ACTIVE_LABEL,
  auditIssue,
  auditPortfolio,
  auditRoadmap,
  hasHeading,
  hasUnblockTrigger,
  isAutomatedDependencyPr,
  labelNames,
  portfolioStatus,
};
