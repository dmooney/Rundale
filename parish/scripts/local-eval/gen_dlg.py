#!/usr/bin/env python3
"""Generate dialogue samples from a vllm-mlx model on identical prompts.

Usage:
  python3 gen_dlg.py <model> <output_path>
"""
import json
import sys
import urllib.request

MODEL = sys.argv[1]
OUT = sys.argv[2]

SYSTEM = (
    "You are Brigid O'Brien, a 42-year-old midwife in rural Ireland, 1820. "
    "You are kind but direct, with a deep knowledge of local plants and folk "
    "medicine. You have known the player's family for years.\n\n"
    "Stay in character. Speak in 1-3 sentences. Do not use modern language."
)

PROMPTS = [
    "I have been having trouble sleeping. The dreams keep coming back.",
    "What do you know about the old Cailleach who lives near the fairy fort?",
    "My mother is taken with a bad cough. Is there anything you can give her?",
    "They say a stranger arrived in the village. Have you heard?",
    "I lost a sheep last night. Could it be more than a wolf?",
]


def call(prompt: str) -> str:
    body = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt},
        ],
        "stream": True,
        "max_tokens": 120,
        "temperature": 0.7,
    }
    req = urllib.request.Request(
        "http://localhost:8000/v1/chat/completions",
        data=json.dumps(body).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    chunks = []
    with urllib.request.urlopen(req, timeout=60) as resp:
        for line in resp:
            line = line.decode("utf-8").strip()
            if not line.startswith("data: "):
                continue
            data_part = line[len("data: "):]
            if data_part == "[DONE]":
                break
            try:
                obj = json.loads(data_part)
                delta = obj["choices"][0].get("delta", {}).get("content")
                if delta:
                    chunks.append(delta)
            except Exception:
                pass
    return "".join(chunks)


with open(OUT, "w") as f:
    f.write(f"=== Model: {MODEL} ===\n")
    for p in PROMPTS:
        text = call(p).strip()
        f.write(f"\nPROMPT: {p}\nREPLY:  {text}\n")
print(f"wrote {OUT}")
