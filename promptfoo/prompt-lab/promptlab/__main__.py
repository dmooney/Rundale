"""Prompt lab CLI. Run from anywhere: python3 promptfoo/prompt-lab/lab.py <command>."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import sys

from . import content, render, research
from . import variant as variant_mod


def _variant(name: str):
    return render.BASELINE if name == render.BASELINE else variant_mod.load_named(name)


def cmd_render(args) -> int:
    case = next(c for c in content.all_cases() if c.id == args.case)
    request = render.render(_variant(args.variant), case)
    print("=== system instruction ===")
    print(request.system)
    print("=== user ===")
    print(
        request.user
        if not args.pretty
        else "Endpoint input values:\n" + json.dumps(request.values, ensure_ascii=False, indent=2)
    )
    print(f"=== {request.size_chars} characters ===")
    return 0


def cmd_estimate(args) -> int:
    """Rough cost of one round from prompt size (4 characters a token)."""
    cfg = research.load_config()
    train = content.cases("train")
    v = _variant(args.variant)
    chars = sum(render.render(v, c).size_chars for c in train)
    samples = cfg["research"]["samples_per_case"]
    total = 0.0
    for name, mcfg in research.model_configs(cfg, args.stand_in).items():
        pin, pout = research.models.price_of(mcfg)
        usd = (chars / 4 * pin + len(train) * 200 * pout) / 1_000_000 * samples
        total += usd
        print(f"{name} ({mcfg['model']}): ~${usd:.4f} per round")
    print(
        f"total ~${total:.4f} per round for {len(train)} cases; cap ${cfg['research']['spend_cap_usd']:.2f}"
    )
    return 0


def cmd_round(args) -> int:
    """Score one prompt on chosen training cases (a one-off, outside the loop)."""
    cfg = research.load_config()
    cases = content.cases(
        "train",
        tags=set(args.tags) if args.tags else None,
        ids=set(args.cases) if args.cases else None,
    )
    run = args.out or f"adhoc-{args.variant}-{dt.datetime.now().strftime('%Y%m%d-%H%M%S')}"
    ledger = research.Ledger(cfg["research"]["spend_cap_usd"])
    summary = research.score_round(
        _variant(args.variant),
        cases,
        cfg,
        ledger,
        research.OUTPUT_DIR / run,
        stand_in=args.stand_in,
    )
    print(
        json.dumps(
            {
                k: summary[k]
                for k in (
                    "score",
                    "by_model",
                    "by_requirement",
                    "failed_replies",
                    "unjudged",
                    "flagged",
                    "round_spend_usd",
                )
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def cmd_loop(args) -> int:
    run = (
        args.run
        or f"{dt.date.today().isoformat()}-{args.start}{'-stand-in' if args.stand_in else ''}"
    )
    state = research.run_loop(
        run,
        args.start,
        stand_in=args.stand_in,
        case_ids=set(args.cases) if args.cases else None,
        max_rounds=args.max_rounds,
    )
    print(
        f"run {run}: best {state['best']}; stopped: {state['stopped']}; paused: {state.get('paused')}; spend ${state['spend_usd']:.4f}"
    )
    return 0


def cmd_heldout(args) -> int:
    research.run_heldout(args.run)
    return 0


def cmd_report(args) -> int:
    text = research.report(args.run)
    if args.write:
        with open(args.write, "w", encoding="utf-8") as handle:
            handle.write(text)
        print(f"wrote {args.write}")
    else:
        sys.stdout.write(text)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="lab.py", description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("render", help="print the exact request a case produces")
    p.add_argument("variant", help="'baseline' or a directory under prompts/")
    p.add_argument("case")
    p.add_argument(
        "--pretty", action="store_true", help="indent the JSON (production sends it compact)"
    )
    p.set_defaults(func=cmd_render)

    p = sub.add_parser("estimate", help="rough model cost of one round")
    p.add_argument("variant", nargs="?", default="brief")
    p.add_argument("--stand-in", action="store_true")
    p.set_defaults(func=cmd_estimate)

    p = sub.add_parser("round", help="score one prompt on training cases, outside the loop")
    p.add_argument("variant")
    p.add_argument("--tags", nargs="*", help="only cases testing these requirements, e.g. B7")
    p.add_argument("--cases", nargs="*", help="only these case ids")
    p.add_argument("--out", help="output directory name under output/")
    p.add_argument(
        "--stand-in", action="store_true", help="plumbing check with the OpenAI stand-in"
    )
    p.set_defaults(func=cmd_round)

    p = sub.add_parser("loop", help="run (or resume) the automated research loop")
    p.add_argument("--run", help="run id under output/ (resumes if it exists)")
    p.add_argument("--start", default="brief", help="starting variant under prompts/")
    p.add_argument("--cases", nargs="*", help="only these training case ids (plumbing checks)")
    p.add_argument("--max-rounds", type=int, help="override max research rounds")
    p.add_argument(
        "--stand-in", action="store_true", help="plumbing check with the OpenAI stand-in"
    )
    p.set_defaults(func=cmd_loop)

    p = sub.add_parser("heldout", help="run the best prompt and the baseline on the held-out set")
    p.add_argument("run")
    p.set_defaults(func=cmd_heldout)

    p = sub.add_parser("report", help="the write-up for docs/test-plans/results/")
    p.add_argument("run")
    p.add_argument("--write", help="write to this path instead of stdout")
    p.set_defaults(func=cmd_report)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
