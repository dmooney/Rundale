export default function bypassBranch(input: string): string | null {
  if (!/if\s*\(/.test(input)) return null;
  return input.replace(/if\s*\(([^)]+)\)/, "if (true)");
}
