import { collectContext } from "../src/context";
import { writeJson } from "../src/utils";
writeJson(".quality-harness/context.json", collectContext());
