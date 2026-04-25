#!/usr/bin/env python3
"""
Rundale LOC Growth Projector
=============================
Reads daily net-LOC data live from git history, fits a growth model
with diminishing returns, and projects milestones.

Excludes markdown (.md) files — code-only count.
"""

import math
import subprocess
from datetime import date, timedelta

CODE_EXTENSIONS = "*.rs *.ts *.svelte *.js *.json *.toml *.css *.html *.sh *.py *.txt"

def git_daily_loc():
    """Run git log to get daily net LOC for code files (excludes .md)."""
    exts = CODE_EXTENSIONS.split()
    cmd = ["git", "log", "--all", "--format=%ad", "--date=short", "--numstat", "--"] + exts
    result = subprocess.run(cmd, capture_output=True, text=True)
    added = {}
    deleted = {}
    current_date = None
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) == 1 and len(parts[0]) == 10 and parts[0][4] == '-':
            current_date = parts[0]
        elif len(parts) >= 3 and current_date and parts[0] != '-':
            added[current_date] = added.get(current_date, 0) + int(parts[0])
            deleted[current_date] = deleted.get(current_date, 0) + int(parts[1])
    days = sorted(added.keys())
    return [(d, added[d] - deleted.get(d, 0)) for d in days]

def git_commit_count():
    result = subprocess.run(["git", "log", "--all", "--oneline"], capture_output=True, text=True)
    return len(result.stdout.strip().splitlines())

ACTUAL = git_daily_loc()
START_DATE = date.fromisoformat(ACTUAL[0][0]) if ACTUAL else date.today()
CURRENT_LOC = sum(net for _, net in ACTUAL)
CURRENT_DAY = (date.fromisoformat(ACTUAL[-1][0]) - START_DATE).days if ACTUAL else 0
COMMIT_COUNT = git_commit_count()

MILESTONES = [100_000, 250_000, 500_000, 1_000_000]

# ── Pretty printing ──────────────────────────────────────────────

BOLD = "\033[1m"
DIM = "\033[2m"
GREEN = "\033[32m"
CYAN = "\033[36m"
YELLOW = "\033[33m"
MAGENTA = "\033[35m"
RESET = "\033[0m"
BAR_CHAR = "█"
BAR_HALF = "▌"


def fmt_loc(n):
    if n >= 1_000_000:
        return f"{n/1_000_000:.1f}M"
    if n >= 1_000:
        return f"{n/1_000:.1f}k"
    return str(n)


def bar(value, max_val, width=40):
    filled = value / max_val * width
    full = int(filled)
    half = 1 if filled - full >= 0.5 else 0
    return BAR_CHAR * full + (BAR_HALF if half else "") + " " * (width - full - half)


# ── Historical summary ───────────────────────────────────────────

print(f"\n{BOLD}Rundale LOC Growth Projection{RESET} {DIM}(code only, no markdown){RESET}")
print(f"{'=' * 55}")
print(f"\n{CYAN}Historical daily net LOC:{RESET}\n")

max_abs = max(abs(d[1]) for d in ACTUAL) if ACTUAL else 1
cumulative = 0
for datestr, net in ACTUAL:
    cumulative += net
    day_num = (date.fromisoformat(datestr) - START_DATE).days
    if net >= 0:
        b = bar(net, max_abs, 30)
        print(f"  Day {day_num:2d} {DIM}{datestr}{RESET}  {GREEN}{b}{RESET} {net:>+7,d}  ({fmt_loc(cumulative)})")
    else:
        b = bar(abs(net), max_abs, 30)
        print(f"  Day {day_num:2d} {DIM}{datestr}{RESET}  {MAGENTA}{b}{RESET} {net:>+7,d}  ({fmt_loc(cumulative)})")

elapsed = CURRENT_DAY + 1
print(f"\n  {BOLD}Current: {CURRENT_LOC:,} LOC on day {CURRENT_DAY}{RESET}")
print(f"  {BOLD}Commits: {COMMIT_COUNT:,} across {elapsed} days{RESET}")


# ── Projection scenarios ─────────────────────────────────────────

def project(name, daily_fn):
    loc = CURRENT_LOC
    results = {}
    for m in MILESTONES:
        if loc >= m:
            results[m] = (CURRENT_DAY, START_DATE + timedelta(days=CURRENT_DAY))
    for day in range(CURRENT_DAY + 1, CURRENT_DAY + 1100):
        loc += daily_fn(day)
        for m in MILESTONES:
            if m not in results and loc >= m:
                results[m] = (day, START_DATE + timedelta(days=day))
        if len(results) == len(MILESTONES):
            break
    return results


positive_days = [n for _, n in ACTUAL if n > 0]
recent = positive_days[-7:]
avg_recent = sum(recent) / len(recent) if recent else 5000

def scenario_ramp(day):
    peak = avg_recent * 1.5
    ramp = 1 - math.exp(-0.08 * day)
    decay = math.exp(-0.003 * (day - 30)) if day > 30 else 1.0
    return peak * ramp * decay * 0.85

def scenario_hyper(day):
    peak = avg_recent * 2.5
    ramp = 1 - math.exp(-0.12 * day)
    decay = math.exp(-0.001 * (day - 45)) if day > 45 else 1.0
    return peak * ramp * decay * 0.85

def scenario_mature(day):
    peak = avg_recent
    decay = math.exp(-0.01 * day)
    maintenance = 500
    return max(peak * decay, maintenance)


scenarios = [
    ("Steady pace", f"~{fmt_loc(int(avg_recent))}/day flat",
     lambda d: avg_recent * 0.85),
    ("Ramp + decay", "peak ~day 30, gradual slowdown",
     scenario_ramp),
    ("Hypergrowth", "AI-assisted, sustained high output",
     scenario_hyper),
    ("Early plateau", "rapid decay to maintenance mode",
     scenario_mature),
]

print(f"\n{BOLD}{'─' * 55}")
print(f"Projection Scenarios{RESET}")
print(f"{'─' * 55}\n")

for name, desc, fn in scenarios:
    results = project(name, fn)
    print(f"  {YELLOW}{BOLD}{name}{RESET} {DIM}({desc}){RESET}")
    for m in MILESTONES:
        if m in results:
            day, dt = results[m]
            remaining = day - CURRENT_DAY
            if remaining <= 0:
                eta = "already reached"
            elif remaining < 7:
                eta = f"in {remaining}d"
            else:
                eta = f"in ~{remaining / 7:.0f} weeks"
            print(f"    {fmt_loc(m):>5s} LOC  →  {CYAN}{dt.strftime('%b %d, %Y')}{RESET}"
                  f"  {DIM}({eta}){RESET}")
        else:
            print(f"    {fmt_loc(m):>5s} LOC  →  {DIM}beyond 3 years{RESET}")
    print()

# ── Fun stats ─────────────────────────────────────────────────────

avg_all = CURRENT_LOC / elapsed if elapsed else 0
lines_per_hour = avg_all / 16

print(f"{BOLD}{'─' * 55}")
print(f"Fun Stats{RESET}")
print(f"{'─' * 55}\n")
print(f"  Average net output:    {BOLD}{avg_all:,.0f}{RESET} LOC/day")
print(f"  That's roughly:        {BOLD}{lines_per_hour:,.0f}{RESET} LOC/hour")
print(f"  Or:                    {BOLD}{lines_per_hour/60:,.1f}{RESET} LOC/minute")
if ACTUAL:
    peak_day = max(ACTUAL, key=lambda x: x[1])
    min_day = min(ACTUAL, key=lambda x: x[1])
    print(f"  Peak single day:       {BOLD}+{peak_day[1]:,}{RESET} LOC ({peak_day[0]})")
    print(f"  Biggest refactor:      {BOLD}{min_day[1]:,}{RESET} LOC ({min_day[0]})")
if avg_all > 0:
    print(f"  Days to write a novel: {DIM}(~80k words){RESET} {BOLD}{80000/avg_all:.1f}{RESET} days at this pace")
print()
