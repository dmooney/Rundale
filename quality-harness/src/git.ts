import { spawnSync } from "child_process";

export function git(args: string[]): string {
  const out = spawnSync("git", args, { encoding: "utf8" });
  if (out.status !== 0) throw new Error(out.stderr || `git ${args.join(" ")} failed`);
  return out.stdout.trim();
}

export const getChangedFiles = (base: string, head: string) => git(["diff", "--name-status", `${base}...${head}`]);
