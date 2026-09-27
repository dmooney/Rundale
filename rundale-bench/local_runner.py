#!/usr/bin/env python3
"""Local MLX candidate runner — spawns mlx_lm.server, runs the bench, kills it.

For each candidate in candidates_local_mlx.toml:
  1. Spawn `mlx_lm.server --model <hf_repo> --host 127.0.0.1 --port <port>`.
  2. Wait until /v1/models responds 200 (model loaded).
  3. Invoke `rundale_bench.py --target <model>@http://127.0.0.1:<port> --slice <slice>`
     in a subprocess; pipe stdout into the captured log.
  4. Sample RSS of the server pid (+ children) every 250 ms during the bench;
     record the peak across the run.
  5. SIGTERM the server, wait, then SIGKILL if still alive.
  6. Append one row to `rundale-bench/artifacts/local_leaderboard.md` and write
     a full result JSON under `rundale-bench/artifacts/local_<utc>.json`.

The runner deliberately invokes the existing `rundale_bench.py` orchestrator
instead of duplicating its grader logic — that keeps local + cloud sweeps
running through one code path.

Run::

    .venv-mlx/bin/python3 rundale-bench/local_runner.py \\
        --slot tiny --slice intent --limit 10

    .venv-mlx/bin/python3 rundale-bench/local_runner.py \\
        --candidates Qwen3-1.7B-4bit,Qwen3-4B-4bit --slice all
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from collections import defaultdict
from collections.abc import Callable
from datetime import datetime, timezone
from pathlib import Path

import tomllib
from candidates_schema import candidates_to_delete, validate_candidates

# psutil lives in .venv-mlx, not the CI dev venv. Import it lazily: the module
# stays importable (so the pure helpers — ram_cap_exceeded, slice_cost_ledger,
# fitness_check, metric_from_summary, validate_candidates — are unit-testable
# without it) and only the live-sampling paths require it. `_require_psutil`
# raises a clear error at the point of use.
try:
    import psutil
except ImportError:  # pragma: no cover - exercised only outside .venv-mlx
    psutil = None


def _require_psutil() -> None:
    """Fail loudly only when a function that genuinely needs psutil runs."""
    if psutil is None:
        sys.exit("psutil missing — run inside .venv-mlx (psutil is bundled there).")


_HERE = Path(__file__).resolve().parent
_REPO_ROOT = _HERE.parent
_ARTIFACTS_DIR = _HERE / "artifacts"
_LEADERBOARD = _ARTIFACTS_DIR / "local_leaderboard.md"
_CANDIDATES_TOML = _HERE / "candidates_local_mlx.toml"
_BENCH_PY = _HERE / "rundale_bench.py"

# Resolve the mlx-lm server binary from the venv that bundles it.
# Resolve the mlx-lm server binary from a venv co-located with the repo.
# Override with `MLX_VENV=/abs/path` if the venv lives elsewhere.
_VENV = Path(os.environ.get("MLX_VENV") or (_REPO_ROOT / ".venv-mlx"))
_MLX_SERVER = _VENV / "bin" / "mlx_lm.server"


def _load_dotenv(path: Path) -> None:
    """Minimal `.env` loader — KEY=VALUE per line, skip blanks/`#` comments.
    Existing environment values win so an explicitly-set shell var is not clobbered.
    """
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        val = val.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = val


_load_dotenv(_REPO_ROOT / ".env")


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def pick_free_port(start: int = 8765) -> int:
    """Return the first free TCP port at or above `start`."""
    for port in range(start, start + 200):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("127.0.0.1", port))
                return port
            except OSError:
                continue
    raise RuntimeError(f"no free port in [{start}, {start + 200})")


def wait_for_ready(base_url: str, timeout_s: float = 600.0) -> None:
    """Poll <base_url>/models until 200; raise on timeout."""
    deadline = time.time() + timeout_s
    last_err = ""
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"{base_url}/models", timeout=3.0) as r:
                if r.status == 200:
                    return
        except (urllib.error.URLError, urllib.error.HTTPError, OSError) as e:
            last_err = str(e)
        time.sleep(1.0)
    raise RuntimeError(f"server at {base_url} not ready in {timeout_s:.0f}s: {last_err}")


def ram_cap_exceeded(peak_rss_bytes: int, max_ram_gb: float | None) -> bool:
    """True when ``peak_rss_bytes`` exceeds the ``max_ram_gb`` ceiling.

    ``max_ram_gb is None`` (the default) disables the cap and always returns
    False, so the kill switch is opt-in. The comparison uses the same 1e9
    bytes-per-GB convention as ``available_memory_gb`` / the leaderboard row.
    """
    if max_ram_gb is None:
        return False
    return peak_rss_bytes / 1e9 > max_ram_gb


class RamSampler:
    """Threaded RSS sampler for a server pid + its descendants.

    Returns peak RSS in bytes across the lifetime of the sampler. On macOS,
    mlx_lm.server is a single Python process — child workers are uncommon —
    but we follow children defensively in case the user runs a vllm-backed
    spawn here later.
    """

    def __init__(
        self,
        pid: int,
        interval_s: float = 0.25,
        max_ram_gb: float | None = None,
        on_breach: Callable[[int], None] | None = None,
    ):
        self.pid = pid
        self.interval_s = interval_s
        # Optional hard ceiling (GB). When a sample's total RSS exceeds it, the
        # sampler sets `breached` and invokes `on_breach(peak_rss)` once so the
        # caller can SIGKILL the server before OOM takes down Claude Code.
        self.max_ram_gb = max_ram_gb
        self._on_breach = on_breach
        self._stop = threading.Event()
        self._peak_rss = 0
        self._samples = 0
        self._breached = False
        self._thread = threading.Thread(target=self._loop, daemon=True)

    @property
    def breached(self) -> bool:
        """True once a sample exceeded ``max_ram_gb`` (latched, never reset)."""
        return self._breached

    @property
    def peak_rss(self) -> int:
        return self._peak_rss

    def _process_memory(self, p: psutil.Process) -> int:
        """Memory bytes for one process, preferring `phys_footprint` on macOS.

        On Apple Silicon, Metal GPU allocations land in `phys_footprint`
        (via `task_info(TASK_VM_INFO)`) but NOT in `rss`. Using rss alone
        undercounts mlx_lm.server memory by ~80% — a 9 GB Qwen2.5-14B-4bit
        reads as 2 GB. `memory_full_info().uss` on darwin maps to
        phys_footprint when available; fall back to rss elsewhere.
        """
        try:
            info = p.memory_full_info()
            # uss = unique set size; on darwin psutil derives this from
            # phys_footprint when running with the process owner's privs.
            val = getattr(info, "uss", 0) or 0
            if val:
                return val
        except (psutil.AccessDenied, AttributeError):
            pass
        return p.memory_info().rss

    def _loop(self) -> None:
        try:
            proc = psutil.Process(self.pid)
        except psutil.NoSuchProcess:
            return
        while not self._stop.is_set():
            try:
                family = [proc] + proc.children(recursive=True)
                total = sum(self._process_memory(p) for p in family if p.is_running())
                if total > self._peak_rss:
                    self._peak_rss = total
                self._samples += 1
                if not self._breached and ram_cap_exceeded(total, self.max_ram_gb):
                    self._breached = True
                    if self._on_breach is not None:
                        self._on_breach(total)
                    break  # ceiling hit; stop sampling, caller kills the server
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                break
            time.sleep(self.interval_s)

    def start(self) -> None:
        _require_psutil()
        self._thread.start()

    def stop(self) -> tuple[int, int]:
        self._stop.set()
        self._thread.join(timeout=2.0)
        return self._peak_rss, self._samples


def spawn_server(hf_repo: str, port: int, log_path: Path) -> subprocess.Popen:
    """Launch mlx_lm.server. Log to `log_path`. Returns the Popen handle."""
    if not _MLX_SERVER.exists():
        raise RuntimeError(f"mlx_lm.server missing at {_MLX_SERVER}; install mlx-lm in .venv-mlx")
    log_fh = open(log_path, "wb")  # noqa: SIM115 -- handle outlives this fn; the spawned server writes to it
    cmd = [
        str(_MLX_SERVER),
        "--model",
        hf_repo,
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
    ]
    proc = subprocess.Popen(
        cmd,
        stdout=log_fh,
        stderr=subprocess.STDOUT,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )
    return proc


def stop_server(proc: subprocess.Popen, grace_s: float = 5.0) -> None:
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=grace_s)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=grace_s)


def run_bench_subproc(
    target_spec: str,
    slice_name: str,
    judge: str,
    split: str,
    limit: int | None,
    log_path: Path,
) -> tuple[int, dict]:
    """Invoke rundale_bench.py and parse the JSON output it writes."""
    args = [
        sys.executable,
        str(_BENCH_PY),
        "--target",
        target_spec,
        "--slice",
        slice_name,
        "--judge",
        judge,
        "--split",
        split,
    ]
    if limit is not None:
        args += ["--limit", str(limit)]
    started = time.time()
    with open(log_path, "ab") as log_fh:
        rc = subprocess.call(args, stdout=log_fh, stderr=subprocess.STDOUT)
    elapsed = time.time() - started
    return rc, {"elapsed_s": elapsed, "rc": rc}


def latest_bench_run(slice_name: str, model_slug: str, since_ts: float) -> Path | None:
    """Return the most-recent run_<model_slug>_<slice>_*.json mtime > since_ts."""
    candidates = []
    for p in _ARTIFACTS_DIR.glob(f"run_{model_slug}_{slice_name}_*.json"):
        st = p.stat()
        if st.st_mtime >= since_ts:
            candidates.append((st.st_mtime, p))
    if not candidates:
        return None
    return max(candidates)[1]


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", s).strip("_")[:80]


def parse_candidates(path: Path) -> list[dict]:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return data.get("candidate", [])


def fitness_check(cand: dict, available_gb: float, headroom_gb: float) -> str | None:
    est = float(cand.get("peak_ram_gb_est", 0.0))
    if est > available_gb - headroom_gb:
        return f"est {est:.1f} GB > available {available_gb:.1f} GB minus {headroom_gb} GB headroom"
    return None


def append_leaderboard_row(row: dict) -> None:
    """Append a markdown row to leaderboard.md under the 'Local MLX sweeps' section."""
    header = "## Local MLX sweeps"
    columns = "| Date (UTC) | hf_repo | slot | quant | params_B | peak_RAM_GB | slice | split | metric | $/run | judge | harness_sha |"
    sep = "|---|---|---|---|---|---|---|---|---|---|---|---|"

    body = _LEADERBOARD.read_text(encoding="utf-8") if _LEADERBOARD.exists() else ""
    if header not in body:
        body += f"\n\n{header}\n\nLocal MLX runs via `local_runner.py`. `peak_RAM_GB` is the live-sampled RSS peak of the mlx_lm.server pid and children. `params_B` is total parameters in billions (with active count for MoE).\n\n{columns}\n{sep}\n"

    # Find the table block and append the row at the end.
    line = (
        f"| {row['date']} | {row['hf_repo']} | {row['slot']} | {row['quant']} | {row['params_b']:.1f}"
        f"{(' (' + str(row['moe_active_b']) + ' active)') if row.get('moe_active_b') else ''}"
        f" | {row['peak_ram_gb']:.2f} | {row['slice']} | {row['split']} | {row['metric']} "
        f"| ${row['cost_usd']:.4f} | {row['judge']} | {row['harness_sha']} |"
    )
    body = body.rstrip() + "\n" + line + "\n"
    _LEADERBOARD.write_text(body, encoding="utf-8")


def harness_sha() -> str:
    try:
        out = (
            subprocess.check_output(["git", "rev-parse", "--short", "HEAD"], cwd=_REPO_ROOT)
            .decode()
            .strip()
        )
        return out
    except Exception:
        return "unknown"


def available_memory_gb() -> float:
    _require_psutil()
    return psutil.virtual_memory().total / 1e9


def slice_cost_ledger(rows: list[dict]) -> dict[str, dict[str, float]]:
    """Per-slice cost ledger across the leaderboard rows of a sweep.

    Local sweeps print ``$0.0000`` on every row because the model runs on the
    laptop — but the Sonnet-subagent judge is not free, it is amortised against
    the Claude Code session. This sums ``cost_usd`` per slice and, when rows
    carry a ``judge_compute_minutes`` field, surfaces the judge compute so the
    hidden cost is legible. Returns ``{slice: {usd, runs, judge_minutes}}`` plus
    a ``"total"`` rollup.
    """
    ledger: dict[str, dict[str, float]] = defaultdict(
        lambda: {"usd": 0.0, "runs": 0.0, "judge_minutes": 0.0}
    )
    for row in rows:
        slice_name = str(row.get("slice", "unknown"))
        ledger[slice_name]["usd"] += float(row.get("cost_usd", 0.0) or 0.0)
        ledger[slice_name]["runs"] += 1
        ledger[slice_name]["judge_minutes"] += float(row.get("judge_compute_minutes", 0.0) or 0.0)

    out: dict[str, dict[str, float]] = {k: dict(v) for k, v in ledger.items()}
    out["total"] = {
        "usd": round(sum(v["usd"] for v in out.values()), 6),
        "runs": sum(v["runs"] for v in out.values()),
        "judge_minutes": round(sum(v["judge_minutes"] for v in out.values()), 4),
    }
    return out


def metric_from_summary(summary: dict) -> str:
    """Compact one-cell metric for the leaderboard row, per slice family.

    Bundled-judge slices (reaction, tier2-sim, tier3-sim, gaeilge) return
    None for score fields when their pending_judge flag is set. Coalesce
    None → 0 so the format string below doesn't TypeError mid-sweep.
    """

    def _f(key: str, default: float = 0.0) -> float:
        v = summary.get(key, default)
        return v if v is not None else default

    s = summary.get("slice")
    if s == "intent":
        return f"label_match={_f('label_match_rate'):.3f}"
    if s == "dialogue":
        return (
            f"overall={_f('overall'):.2f} "
            f"(c={_f('character'):.1f}/"
            f"a={_f('authenticity'):.1f}/"
            f"l={_f('language'):.1f}/"
            f"r={_f('responsiveness'):.1f}/"
            f"cr={_f('craft'):.1f})"
        )
    if s == "reaction":
        pending = " (pending_judge)" if summary.get("pending_judge") else ""
        return f"mean_in_character={_f('mean_score'):.2f}{pending}"
    if s in ("tier2-sim", "tier3-sim"):
        pending = " (pending_judge)" if summary.get("pending_judge") else ""
        return (
            f"schema_valid={_f('schema_valid_rate'):.2f} "
            f"plausibility={_f('mean_score'):.2f}{pending}"
        )
    if s == "gaeilge":
        pending = " (pending_judge)" if summary.get("pending_judge") else ""
        return f"gaeilge_overall={_f('overall'):.2f}{pending}"
    return "n/a"


def main() -> None:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--slot", default=None, choices=["tiny", "large"], help="filter to one slot")
    ap.add_argument(
        "--candidates",
        default=None,
        help="comma-separated short names (last segment of hf_repo) to include; default = all in slot",
    )
    ap.add_argument(
        "--slice",
        default="dialogue",
        choices=["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim", "all"],
    )
    ap.add_argument("--split", default="dev", choices=["dev", "holdout"])
    ap.add_argument("--limit", type=int, default=10, help="prompts per slice")
    ap.add_argument(
        "--judge",
        default="judge_sonnet_v1",
        help="judge config id; Sonnet-subagent ONLY (judge_v1 + other "
        "HTTP-API configs are refused at load time).",
    )
    ap.add_argument(
        "--headroom-gb",
        type=float,
        default=4.0,
        help="GB of unified memory to leave free for OS/other apps",
    )
    ap.add_argument(
        "--max-ram-gb",
        type=float,
        default=None,
        help="runtime RAM-cap kill switch: if live peak RSS exceeds N GB, "
        "SIGKILL the mlx_lm.server and skip the candidate (defends against an "
        "OOM that would kill Claude Code). Default: disabled.",
    )
    ap.add_argument(
        "--dry-run", action="store_true", help="print the plan without spawning servers"
    )
    args = ap.parse_args()

    if not _MLX_SERVER.exists():
        sys.exit(f"mlx_lm.server missing at {_MLX_SERVER}; install mlx-lm in {_VENV}")

    all_cands = parse_candidates(_CANDIDATES_TOML)
    schema_problems = validate_candidates(all_cands)
    if schema_problems:
        for p in schema_problems:
            print(f"[schema] {p}", file=sys.stderr)
        sys.exit(f"{_CANDIDATES_TOML.name} failed schema validation; fix before running")
    if args.slot:
        all_cands = [c for c in all_cands if c["slot"] == args.slot]
    if args.candidates:
        wanted = {n.strip() for n in args.candidates.split(",")}
        all_cands = [c for c in all_cands if c["hf_repo"].split("/")[-1] in wanted]

    avail_gb = available_memory_gb()
    print(f"# total memory: {avail_gb:.1f} GB, headroom: {args.headroom_gb} GB")
    print(f"# {len(all_cands)} candidate(s) selected:")
    for c in all_cands:
        msg = fitness_check(c, avail_gb, args.headroom_gb)
        flag = f"  SKIP — {msg}" if msg else ""
        print(f"  - {c['hf_repo']} ({c['quant']}, ~{c['peak_ram_gb_est']} GB){flag}")

    if args.dry_run:
        return

    _ARTIFACTS_DIR.mkdir(parents=True, exist_ok=True)
    sha = harness_sha()
    slices = (
        ["intent", "dialogue", "reaction", "tier2-sim", "tier3-sim"]
        if args.slice == "all"
        else [args.slice]
    )

    summary_log: list[dict] = []
    for c in all_cands:
        msg = fitness_check(c, avail_gb, args.headroom_gb)
        if msg:
            print(f"[skip] {c['hf_repo']}: {msg}")
            continue

        stamp = utc_stamp()
        log_dir = _ARTIFACTS_DIR / "local_logs"
        log_dir.mkdir(exist_ok=True)
        server_log = log_dir / f"{slug(c['hf_repo'])}_{stamp}_server.log"
        bench_log = log_dir / f"{slug(c['hf_repo'])}_{stamp}_bench.log"

        port = pick_free_port()
        base_url = f"http://127.0.0.1:{port}/v1"
        target_spec = f"{c['hf_repo']}@{base_url}"

        print(f"\n=== {c['hf_repo']} on port {port} ===")
        proc = spawn_server(c["hf_repo"], port, server_log)
        try:
            print(f"[server] pid={proc.pid}, waiting for model to load …")
            wait_for_ready(base_url, timeout_s=900.0)
            print("[server] ready")

            # Arm the runtime RAM-cap kill switch. When a sample crosses
            # --max-ram-gb the sampler latches `breached`, SIGKILLs the server
            # immediately (on_breach), and we abort this candidate's remaining
            # slices before the OS OOM-killer can take down Claude Code.
            def _kill_on_breach(peak: int, _proc: subprocess.Popen = proc) -> None:
                print(
                    f"[ram-cap] peak {peak / 1e9:.2f} GB > --max-ram-gb "
                    f"{args.max_ram_gb}; SIGKILL server pid={_proc.pid}",
                    file=sys.stderr,
                )
                _proc.kill()

            sampler = RamSampler(
                proc.pid,
                max_ram_gb=args.max_ram_gb,
                on_breach=_kill_on_breach if args.max_ram_gb is not None else None,
            )
            sampler.start()

            t0 = time.time()
            for s in slices:
                if sampler.breached:
                    print(
                        f"[ram-cap] skipping remaining slices for {c['hf_repo']} "
                        f"(peak exceeded {args.max_ram_gb} GB)"
                    )
                    break
                print(f"[bench] running slice={s} split={args.split} limit={args.limit}")
                rc, info = run_bench_subproc(
                    target_spec=target_spec,
                    slice_name=s,
                    judge=args.judge,
                    split=args.split,
                    limit=args.limit,
                    log_path=bench_log,
                )
                if rc != 0:
                    print(f"[bench] slice={s} failed rc={rc}; see {bench_log}")
                    continue

                # Locate the JSON the orchestrator just wrote for this slice.
                run_path = latest_bench_run(s, slug(c["hf_repo"]), t0 - 1.0)
                if not run_path:
                    print(f"[bench] no run JSON found for slice={s}")
                    continue

                data = json.loads(run_path.read_text(encoding="utf-8"))
                slc = data.get("slices", {}).get(s, {})
                summary = slc.get("summary", {})

                # Current peak so the slice row reflects the load up to here.
                peak_rss, samples = sampler.peak_rss, sampler._samples

                row = {
                    "date": stamp,
                    "hf_repo": c["hf_repo"],
                    "slot": c["slot"],
                    "quant": c["quant"],
                    "params_b": float(c.get("params_b", 0.0)),
                    "moe_active_b": c.get("moe_active_b"),
                    "peak_ram_gb": peak_rss / 1e9,
                    "slice": s,
                    "split": args.split,
                    "metric": metric_from_summary(summary),
                    "cost_usd": data.get("cost", {}).get("usd", 0.0),
                    "judge": args.judge,
                    "harness_sha": sha,
                    "ram_samples": samples,
                    "elapsed_s": data.get("elapsed_seconds", 0.0),
                    "run_path": str(run_path.relative_to(_REPO_ROOT)),
                }
                append_leaderboard_row(row)
                summary_log.append(row)
                print(
                    f"[done] {s}: {row['metric']}  peak_ram={row['peak_ram_gb']:.2f} GB  ${row['cost_usd']:.4f}"
                )

            sampler.stop()
        finally:
            stop_server(proc)
            print(f"[server] stopped ({c['hf_repo']})")

    out_path = _ARTIFACTS_DIR / f"local_{utc_stamp()}.json"
    out_path.write_text(
        json.dumps(
            {
                "host": {
                    "platform": sys.platform,
                    "machine": platform.machine(),
                    "memory_gb": avail_gb,
                },
                "harness_sha": sha,
                "split": args.split,
                "limit": args.limit,
                "judge": args.judge,
                "max_ram_gb": args.max_ram_gb,
                "rows": summary_log,
                "cost_ledger": slice_cost_ledger(summary_log),
                "delete_after_bench": candidates_to_delete(all_cands),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"\nwrote {out_path.relative_to(_REPO_ROOT)}")


if __name__ == "__main__":
    main()
