import fs from "fs";

export async function aiJson(promptPath: string, payload: unknown): Promise<string> {
  const apiBase = process.env.OPENAI_BASE_URL || "https://api.openai.com/v1";
  const apiKey = process.env.OPENAI_API_KEY;
  const model = process.env.OPENAI_MODEL || "gpt-4.1";
  if (!apiKey) throw new Error("OPENAI_API_KEY missing");
  const prompt = fs.readFileSync(promptPath, "utf8");
  const res = await fetch(`${apiBase}/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${apiKey}` },
    body: JSON.stringify({
      model,
      response_format: { type: "json_object" },
      messages: [
        { role: "system", content: prompt },
        { role: "user", content: JSON.stringify(payload) },
      ],
    }),
  });
  if (!res.ok) throw new Error(`AI call failed: ${res.status}`);
  const json = await res.json() as any;
  return json.choices?.[0]?.message?.content || "{}";
}
