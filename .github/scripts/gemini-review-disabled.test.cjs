const assert = require("node:assert/strict");
const { existsSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");

const workflows = join(process.cwd(), ".github", "workflows");

test("Gemini review workflows remain disabled", () => {
  for (const name of ["gemini-dispatch", "gemini-review"]) {
    assert.equal(
      existsSync(join(workflows, `${name}.yml`)),
      false,
      `${name}.yml would reactivate the Gemini review integration`,
    );
    assert.equal(
      existsSync(join(workflows, `${name}.yml.disabled`)),
      true,
      `${name}.yml.disabled should retain the paused workflow source`,
    );
  }
});
