export default function booleanFlip(input: string): string | null {
  if (input.includes("true")) return input.replace("true", "false");
  if (input.includes("false")) return input.replace("false", "true");
  return null;
}
