export default function comparisonFlip(input: string): string | null {
  const pairs: Array<[string, string]> = [[">=", "<"], ["<=", ">"], ["==", "!="], ["===", "!=="], [">", "<="], ["<", ">="]];
  for (const [a, b] of pairs) if (input.includes(a)) return input.replace(a, b);
  return null;
}
