export default function removeThrow(input: string): string | null {
  if (!/throw\s+/.test(input)) return null;
  return input.replace(/throw\s+[^;]+;/, "");
}
