import fs from "fs";
import { spawnSync } from "child_process";

export interface MutationRun { killed: any[]; survived: any[]; skipped: any[] }

export function runCommand(command: string): number {
  const r = spawnSync(command, { shell: true, stdio: "inherit" });
  return r.status ?? 1;
}

export function withMutation(file: string, mutator: (input: string) => string | null, testCommand: string) {
  const original = fs.readFileSync(file, "utf8");
  const mutated = mutator(original);
  if (!mutated || mutated === original) return { status: "skipped" };
  fs.writeFileSync(file, mutated, "utf8");
  try {
    const code = runCommand(testCommand);
    return { status: code === 0 ? "survived" : "killed" };
  } finally {
    fs.writeFileSync(file, original, "utf8");
  }
}
