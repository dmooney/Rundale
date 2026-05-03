import { Octokit } from "@octokit/rest";

export function parseRepo() {
  const repo = process.env.GITHUB_REPOSITORY || "";
  const [owner, repoName] = repo.split("/");
  if (!owner || !repoName) throw new Error("GITHUB_REPOSITORY missing");
  return { owner, repo: repoName };
}

export function getOctokit() {
  const token = process.env.GITHUB_TOKEN;
  if (!token) throw new Error("GITHUB_TOKEN missing");
  return new Octokit({ auth: token });
}
