#!/usr/bin/env python3
"""Run cheap single-tile image-generation tests against provider APIs.

This is intentionally small and artifact-oriented: it reads one source map
tile, sends the same prompt to selected image models, writes outputs plus
redacted metadata, and builds a comparison sheet.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageFont

GOOGLE_INTERACTIONS_URL = "https://generativelanguage.googleapis.com/v1beta/interactions"
OPENROUTER_IMAGES_URL = "https://openrouter.ai/api/v1/images"


PROMPT = """Use the input image as strict geography/layout authority.

Create one north-up overhead gameplay map tile in muted rural Irish ink and watercolor.
This is a flat map surface, not an isometric scene and not a landscape painting.

Preserve:
- every road/path/boundary line in the same relative position;
- the building footprint and yard relationship;
- the source tile's edge continuity, with features continuing cleanly off-frame.

Style:
- warm parchment paper, soft moss/straw greens, raw umber, fine dark-brown ink;
- subtle watercolor texture and hand-painted irregularity;
- hedge/bank/ditch field boundaries by default, not uniform stone block walls.

Avoid:
- readable labels, numbers, letters, UI, compass, people, animals, carts;
- new buildings, new roads, recentered composition, perspective camera;
- dark grid seams or abrupt edge effects.

Output a single square 1K image suitable to be cropped/split back into a runtime map tile.
"""


@dataclass(frozen=True)
class ModelSpec:
    provider: str
    model: str
    output_cost_usd_1k: float
    note: str


MODEL_SPECS = [
    ModelSpec(
        provider="openrouter",
        model="google/gemini-3.1-flash-lite-image",
        output_cost_usd_1k=0.0336,
        note="OpenRouter route to Google Nano Banana 2 Lite; cheap image-to-image baseline.",
    ),
    ModelSpec(
        provider="openrouter",
        model="sourceful/riverflow-v2.5-fast",
        output_cost_usd_1k=0.019,
        note="OpenRouter route to Sourceful Riverflow fast image-to-image.",
    ),
    ModelSpec(
        provider="openrouter",
        model="black-forest-labs/flux.2-klein-4b",
        output_cost_usd_1k=0.014,
        note="OpenRouter route to FLUX.2 Klein 4B; listed per-megapixel output cost.",
    ),
    ModelSpec(
        provider="openrouter",
        model="google/gemini-3.1-flash-image",
        output_cost_usd_1k=0.067,
        note="OpenRouter route to Google Nano Banana 2; higher cost than Lite.",
    ),
    ModelSpec(
        provider="openrouter",
        model="openai/gpt-image-1-mini",
        output_cost_usd_1k=0.052,
        note="OpenRouter route to GPT Image 1 Mini; cheaper OpenAI image baseline.",
    ),
    ModelSpec(
        provider="openrouter",
        model="openai/gpt-image-1",
        output_cost_usd_1k=0.20,
        note="OpenRouter route to GPT Image 1; higher-cost OpenAI image baseline.",
    ),
    ModelSpec(
        provider="google",
        model="gemini-3.1-flash-lite-image",
        output_cost_usd_1k=0.0336,
        note="Cheapest current Google native image model; optimized for speed/cost.",
    ),
    ModelSpec(
        provider="google",
        model="gemini-3.1-flash-image",
        output_cost_usd_1k=0.067,
        note="Google generalist native image model; higher quality/cost than Lite.",
    ),
    ModelSpec(
        provider="google",
        model="gemini-2.5-flash-image",
        output_cost_usd_1k=0.039,
        note="Legacy Nano Banana image model; included as a backward-compatibility baseline.",
    ),
]


def safe_slug(text: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "._-" else "-" for ch in text).strip("-")


def load_env(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        key = key.strip()
        value = value.strip().strip("'\"")
        if key and key not in os.environ:
            os.environ[key] = value


def safe_response_metadata(response: dict[str, Any]) -> dict[str, Any]:
    def scrub(value: Any) -> Any:
        if isinstance(value, dict):
            out: dict[str, Any] = {}
            for key, child in value.items():
                if key in {"data", "inlineData"}:
                    out[key] = f"<redacted {len(str(child))} chars>"
                else:
                    out[key] = scrub(child)
            return out
        if isinstance(value, list):
            return [scrub(item) for item in value]
        if isinstance(value, str) and len(value) > 512:
            return f"<redacted {len(value)} chars>"
        return value

    return scrub(response)


def extract_output_image(response: dict[str, Any]) -> bytes:
    output_image = response.get("output_image")
    if isinstance(output_image, dict) and output_image.get("data"):
        return base64.b64decode(output_image["data"])

    # Fallback for future response-shape changes: walk the response looking for
    # image-looking dicts that contain base64 data.
    stack: list[Any] = [response]
    while stack:
        value = stack.pop()
        if isinstance(value, dict):
            mime = value.get("mime_type") or value.get("mimeType")
            data = value.get("data")
            if isinstance(data, str) and isinstance(mime, str) and mime.startswith("image/"):
                return base64.b64decode(data)
            stack.extend(value.values())
        elif isinstance(value, list):
            stack.extend(value)
    raise RuntimeError("response did not contain an output image")


def extract_output_mime_type(response: dict[str, Any]) -> str:
    output_image = response.get("output_image")
    if isinstance(output_image, dict) and output_image.get("mime_type"):
        return str(output_image["mime_type"])
    stack: list[Any] = [response]
    while stack:
        value = stack.pop()
        if isinstance(value, dict):
            mime = value.get("mime_type") or value.get("mimeType")
            data = value.get("data")
            if isinstance(data, str) and isinstance(mime, str) and mime.startswith("image/"):
                return mime
            stack.extend(value.values())
        elif isinstance(value, list):
            stack.extend(value)
    return "image/jpeg"


def google_interaction(
    spec: ModelSpec, source: Path, out_dir: Path, timeout_s: int
) -> dict[str, Any]:
    api_key = os.environ.get("GOOGLE_API_KEY") or os.environ.get("GEMINI_API_KEY")
    if not api_key:
        return {"status": "skipped", "error": "missing GOOGLE_API_KEY/GEMINI_API_KEY"}

    image_b64 = base64.b64encode(source.read_bytes()).decode("utf-8")
    payload = {
        "model": spec.model,
        "input": [
            {"type": "text", "text": PROMPT},
            {"type": "image", "mime_type": "image/png", "data": image_b64},
        ],
        "response_format": {
            "type": "image",
            "mime_type": "image/jpeg",
            "aspect_ratio": "1:1",
            "image_size": "1K",
        },
    }
    request = urllib.request.Request(
        GOOGLE_INTERACTIONS_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "x-goog-api-key": api_key,
            "User-Agent": "rundale-graphics-v2-single-tile-tests",
        },
        method="POST",
    )
    started = time.monotonic()
    try:
        with urllib.request.urlopen(request, timeout=timeout_s) as response:
            body = response.read()
        elapsed = time.monotonic() - started
        decoded = json.loads(body)
        output_bytes = extract_output_image(decoded)
        mime_type = extract_output_mime_type(decoded)
        suffix = ".jpg" if mime_type == "image/jpeg" else ".png"
        output_path = out_dir / f"{safe_slug(spec.provider + '-' + spec.model)}{suffix}"
        output_path.write_bytes(output_bytes)
        safe_meta = safe_response_metadata(decoded)
        report = {
            "status": "ok",
            "provider": spec.provider,
            "model": spec.model,
            "output": output_path.name,
            "output_mime_type": mime_type,
            "elapsed_seconds": elapsed,
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "note": spec.note,
            "response_metadata": safe_meta,
        }
    except urllib.error.HTTPError as exc:
        error_body = exc.read().decode("utf-8", "replace")
        report = {
            "status": "error",
            "provider": spec.provider,
            "model": spec.model,
            "http_status": exc.code,
            "error": error_body[:2000],
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "note": spec.note,
        }
    except Exception as exc:  # noqa: BLE001 - artifact runner should record provider failures.
        report = {
            "status": "error",
            "provider": spec.provider,
            "model": spec.model,
            "error": repr(exc),
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "note": spec.note,
        }
    (out_dir / f"{safe_slug(spec.provider + '-' + spec.model)}.report.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
    return report


def openrouter_image(
    spec: ModelSpec, source: Path, out_dir: Path, timeout_s: int
) -> dict[str, Any]:
    api_key = os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        return {"status": "skipped", "error": "missing OPENROUTER_API_KEY"}

    image_b64 = base64.b64encode(source.read_bytes()).decode("utf-8")
    payload: dict[str, Any] = {
        "model": spec.model,
        "prompt": PROMPT,
        "input_references": [
            {
                "type": "image_url",
                "image_url": {"url": f"data:image/png;base64,{image_b64}"},
            }
        ],
        "n": 1,
    }
    if spec.model.startswith("google/"):
        payload["resolution"] = "1K"
        payload["aspect_ratio"] = "1:1"
    elif spec.model.startswith("openai/"):
        payload["quality"] = "medium"
        payload["background"] = "opaque"
    elif spec.model.startswith("sourceful/"):
        payload["resolution"] = "1K"
        payload["output_format"] = "jpeg"
    elif spec.model.startswith("black-forest-labs/"):
        payload["output_format"] = "png"

    request = urllib.request.Request(
        OPENROUTER_IMAGES_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
            "User-Agent": "rundale-graphics-v2-single-tile-tests",
        },
        method="POST",
    )
    started = time.monotonic()
    try:
        with urllib.request.urlopen(request, timeout=timeout_s) as response:
            body = response.read()
        elapsed = time.monotonic() - started
        decoded = json.loads(body)
        image_record = decoded["data"][0]
        output_bytes = base64.b64decode(image_record["b64_json"])
        media_type = image_record.get("media_type") or "image/png"
        suffix = (
            ".jpg"
            if media_type == "image/jpeg"
            else ".svg"
            if media_type == "image/svg+xml"
            else ".png"
        )
        output_path = out_dir / f"{safe_slug(spec.provider + '-' + spec.model)}{suffix}"
        output_path.write_bytes(output_bytes)
        safe_meta = safe_response_metadata(decoded)
        report = {
            "status": "ok",
            "provider": spec.provider,
            "model": spec.model,
            "output": output_path.name,
            "output_mime_type": media_type,
            "elapsed_seconds": elapsed,
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "observed_usage_cost_usd": decoded.get("usage", {}).get("cost"),
            "note": spec.note,
            "response_metadata": safe_meta,
        }
    except urllib.error.HTTPError as exc:
        error_body = exc.read().decode("utf-8", "replace")
        report = {
            "status": "error",
            "provider": spec.provider,
            "model": spec.model,
            "http_status": exc.code,
            "error": error_body[:2000],
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "note": spec.note,
        }
    except Exception as exc:  # noqa: BLE001
        report = {
            "status": "error",
            "provider": spec.provider,
            "model": spec.model,
            "error": repr(exc),
            "listed_output_cost_usd_1k": spec.output_cost_usd_1k,
            "note": spec.note,
        }
    (out_dir / f"{safe_slug(spec.provider + '-' + spec.model)}.report.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
    return report


def label_panel(img: Image.Image, title: str, subtitle: str = "", width: int = 384) -> Image.Image:
    font = ImageFont.load_default()
    scale = width / img.width
    resized = img.resize((width, max(1, int(img.height * scale))), Image.Resampling.LANCZOS)
    header_h = 58 if subtitle else 40
    panel = Image.new("RGB", (width, resized.height + header_h), (244, 241, 232))
    draw = ImageDraw.Draw(panel)
    draw.text((12, 10), title, fill=(35, 31, 24), font=font)
    if subtitle:
        draw.text((12, 31), subtitle, fill=(92, 82, 66), font=font)
    panel.paste(resized.convert("RGB"), (0, header_h))
    return panel


def make_contact_sheet(source: Path, reports: list[dict[str, Any]], out_dir: Path) -> None:
    panels = [
        label_panel(Image.open(source), "Source z17 tile", "strict layout input"),
    ]
    nearest = out_dir / f"{source.stem}-3x-nearest.png"
    if nearest.exists():
        panels.append(
            label_panel(Image.open(nearest), "3x nearest reference", "not art, scale reference")
        )
    for report in reports:
        if report.get("status") == "ok":
            title = f"{report['provider']}: {report['model']}"
            subtitle = f"${report['listed_output_cost_usd_1k']:.4f} listed 1K output"
            panels.append(label_panel(Image.open(out_dir / report["output"]), title, subtitle))
        else:
            blank = Image.new("RGB", (384, 384), (230, 218, 198))
            panels.append(
                label_panel(blank, report["model"], f"error: {report.get('http_status', '')}")
            )

    cols = 2
    gap = 20
    rows = (len(panels) + cols - 1) // cols
    cell_w = max(panel.width for panel in panels)
    cell_h = max(panel.height for panel in panels)
    sheet = Image.new(
        "RGB", (cols * cell_w + (cols + 1) * gap, rows * cell_h + (rows + 1) * gap), (244, 241, 232)
    )
    for index, panel in enumerate(panels):
        x = gap + (index % cols) * (cell_w + gap)
        y = gap + (index // cols) * (cell_h + gap)
        sheet.paste(panel, (x, y))
    sheet.save(out_dir / "single-tile-provider-comparison.png")


def write_readme(out_dir: Path, source: Path, reports: list[dict[str, Any]]) -> None:
    lines = [
        "# Cycle CG Single Tile Provider Imagegen Tests",
        "",
        "Purpose: compare cheaper image-generation models on one real OS 6-inch",
        "z17 map tile before spending money on parish-scale generation.",
        "",
        f"- Source tile: `{source.name}`",
        "- Prompt: `prompt.md`",
        "- Contact sheet: `single-tile-provider-comparison.png`",
        "",
        "## Results",
        "",
        "| Provider | Model | Status | Listed 1K output cost | Output |",
        "| --- | --- | --- | ---: | --- |",
    ]
    for report in reports:
        lines.append(
            "| {provider} | `{model}` | {status} | ${cost:.4f} | {output} |".format(
                provider=report.get("provider", ""),
                model=report.get("model", ""),
                status=report.get("status", ""),
                cost=float(report.get("listed_output_cost_usd_1k", 0)),
                output=report.get("output", ""),
            )
        )
    lines.extend(
        [
            "",
            "## Notes",
            "",
            "- Costs are current listed output prices for 1K images; input image/text",
            "  tokens are additional and should be measured in a larger pilot.",
            "- These are single-tile tests only. They do not prove seam continuity.",
            "- Use the comparison sheet to judge map fidelity, label leakage, and",
            "  whether the model invents/recenters features.",
        ]
    )
    (out_dir / "README.md").write_text("\n".join(lines) + "\n")
    (out_dir / "prompt.md").write_text(PROMPT + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--env-file", type=Path, default=Path("/Users/dmooney/Rundale/.env"))
    parser.add_argument("--timeout-s", type=int, default=180)
    parser.add_argument("--models", nargs="*", default=None)
    parser.add_argument("--providers", nargs="*", default=["openrouter"])
    args = parser.parse_args()

    load_env(args.env_file)
    args.out_dir.mkdir(parents=True, exist_ok=True)
    reports: list[dict[str, Any]] = []
    wanted_models = set(args.models) if args.models else None
    wanted_providers = set(args.providers)
    specs = [
        spec
        for spec in MODEL_SPECS
        if spec.provider in wanted_providers
        and (wanted_models is None or spec.model in wanted_models)
    ]
    for spec in specs:
        if spec.provider == "google":
            reports.append(google_interaction(spec, args.source, args.out_dir, args.timeout_s))
        elif spec.provider == "openrouter":
            reports.append(openrouter_image(spec, args.source, args.out_dir, args.timeout_s))
        else:
            reports.append({"status": "skipped", "provider": spec.provider, "model": spec.model})
    make_contact_sheet(args.source, reports, args.out_dir)
    write_readme(args.out_dir, args.source, reports)
    print(json.dumps({"reports": reports, "out_dir": str(args.out_dir)}, indent=2))
    return 0 if all(report.get("status") == "ok" for report in reports) else 1


if __name__ == "__main__":
    raise SystemExit(main())
