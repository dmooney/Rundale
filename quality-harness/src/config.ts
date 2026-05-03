import fs from "fs";
import path from "path";
import YAML from "yaml";
import { HarnessConfig } from "./types";

const CFG = "quality-harness.yml";
const EXAMPLE = "quality-harness.example.yml";

export function loadConfig(): HarnessConfig {
  const p = fs.existsSync(CFG) ? CFG : EXAMPLE;
  const data = YAML.parse(fs.readFileSync(p, "utf8")) as HarnessConfig;
  if (!data?.project?.name) throw new Error(`Invalid config: ${path.resolve(p)}`);
  return data;
}
