import fs from "fs";
import path from "path";
import { getChangedFiles, git } from "./git";
import { loadConfig } from "./config";
import { safeRead, truncateContent } from "./utils";

export function collectContext() {
  const cfg = loadConfig();
  const base = process.env.GITHUB_BASE_REF || git(["merge-base", "HEAD", "origin/main"]);
  const head = process.env.GITHUB_SHA || "HEAD";
  const changed = getChangedFiles(base, head).split("\n").filter(Boolean).map((l) => {
    const [status, ...rest] = l.split("\t");
    return { status, file: rest[rest.length - 1] };
  });
  const files = changed.map((c) => c.file);
  const changedTests = files.filter((f) => cfg.tests.patterns.some((p) => new RegExp(p).test(f)));
  const deletedTestFiles = changed.filter((c) => c.status.startsWith("D") && changedTests.includes(c.file)).map((c) => c.file);
  const sourceFiles = files.filter((f) => !changedTests.includes(f));
  const docs = cfg.docs.requirements.map((f) => ({ file: f, content: safeRead(f) }));

  return {
    pr: {
      number: Number(process.env.PR_NUMBER || 0),
      title: process.env.PR_TITLE || "",
      body: process.env.PR_BODY || "",
      base,
      head,
    },
    changed_files: changed,
    full_diff: truncateContent(git(["diff", `${base}...${head}`])),
    changed_source_files: sourceFiles,
    changed_test_files: changedTests,
    deleted_test_files: deletedTestFiles,
    nearby_tests: fs.existsSync(".") ? files.filter((f) => cfg.tests.directories.some((d) => path.dirname(f).startsWith(d))) : [],
    docs,
    package_json: safeRead("package.json"),
    risk_areas: cfg.risk_areas,
    config: cfg,
  };
}
