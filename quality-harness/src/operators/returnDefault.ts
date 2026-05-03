export default function returnDefault(input: string): string | null {
  return input.replace(/return\s+[^;]+;/, "return undefined;");
}
