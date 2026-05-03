import fs from "fs";
const audit = JSON.parse(fs.readFileSync(".quality-harness/audit-result.json", "utf8"));
const mutation = fs.existsSync(".quality-harness/mutation-result.json") ? JSON.parse(fs.readFileSync(".quality-harness/mutation-result.json", "utf8")) : { survived: [] };
const rank: Record<string, number> = { pass: 0, warn: 1, fail: 2 };
const min = process.env.MINIMUM_RESULT || "pass";
if (rank[audit.result] < rank[min]) { console.error(`Audit result ${audit.result} below minimum ${min}`); process.exit(1); }
if ((mutation.survived || []).length > 0) { console.error("Mutation survivors found"); process.exit(1); }
