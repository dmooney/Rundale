#!/usr/bin/env python3
"""
Parish LOC Growth Projector
============================
Uses actual daily net-LOC data from git history, fits a growth model
with diminishing returns, and projects milestones.

Model: daily output follows a learning curve that ramps up then
gradually declines as the codebase matures and more time goes to
maintenance vs. greenfield. We use a damped exponential:

    daily_net(t) = peak * (1 - e^(-ramp*t)) * e^(-decay*t)

Cumulative LOC is the running integral of that.
"""

import math
from datetime import date, timedelta

# --- Actual data from git log --all ---
ACTUAL = [
    ("2026-03-18",  8535),
    ("2026-03-19",  2318),
    ("2026-03-20",  2780),
    ("2026-03-21", 11189),
    ("2026-03-22", 10285),
    ("2026-03-23",  3948),
    ("2026-03-24", 26486),
    ("2026-03-25", 37168),
    ("2026-03-26", -31472),  # big refactor day
    ("2026-03-27",  4721),
    ("2026-03-28",  3787),
    ("2026-03-29", 11953),
    ("2026-03-30", 17187),
    ("2026-03-31", -5811),  # big dedup: src/ modules consolidated into parish-core
    ("2026-04-01", -6500),  # continued refactor + slash commands, save picker
]

START_DATE = date(2026, 3, 18)
CURRENT_LOC = 71848
CURRENT_DAY = (date(2026, 4, 2) - START_DATE).days  # day 15

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

print(f"\n{BOLD}Parish LOC Growth Projection{RESET}")
print(f"{'=' * 50}")
print(f"\n{CYAN}Historical daily net LOC:{RESET}\n")

max_abs = max(abs(d[1]) for d in ACTUAL)
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

print(f"\n  {BOLD}Current: {CURRENT_LOC:,} LOC on day {CURRENT_DAY}{RESET}")
print(f"  {BOLD}Commits: 155 across 15 days{RESET}")


# ── Projection scenarios ─────────────────────────────────────────

def project(name, daily_fn):
    """
    Given a function daily_fn(day_number) -> net_loc_for_that_day,
    accumulate from current state and find milestone dates.
    """
    loc = CURRENT_LOC
    results = {}
    for day in range(CURRENT_DAY + 1, CURRENT_DAY + 1100):
        loc += daily_fn(day)
        for m in MILESTONES:
            if m not in results and loc >= m:
                results[m] = (day, START_DATE + timedelta(days=day))
        if len(results) == len(MILESTONES):
            break
    return results


# Scenario 1: Maintain current pace (avg of last 7 non-refactor days)
recent = [n for _, n in ACTUAL[-7:] if n > 0]
avg_recent = sum(recent) / len(recent) if recent else 5000

# Scenario 2: Ramp up with diminishing returns
def scenario_ramp(day):
    peak = avg_recent * 1.5
    ramp = 1 - math.exp(-0.08 * day)
    decay = math.exp(-0.003 * (day - 30)) if day > 30 else 1.0
    return peak * ramp * decay * 0.85

# Scenario 3: Claude-assisted hypergrowth
def scenario_hyper(day):
    peak = avg_recent * 2.5
    ramp = 1 - math.exp(-0.12 * day)
    decay = math.exp(-0.001 * (day - 45)) if day > 45 else 1.0
    return peak * ramp * decay * 0.85

# Scenario 4: Steady state / maturity
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

print(f"\n{BOLD}{'─' * 50}")
print(f"Projection Scenarios{RESET}")
print(f"{'─' * 50}\n")

for name, desc, fn in scenarios:
    results = project(name, fn)
    print(f"  {YELLOW}{BOLD}{name}{RESET} {DIM}({desc}){RESET}")
    for m in MILESTONES:
        if m in results:
            day, dt = results[m]
            weeks = day / 7
            print(f"    {fmt_loc(m):>5s} LOC  →  {CYAN}{dt.strftime('%b %d, %Y')}{RESET}"
                  f"  {DIM}(day {day}, ~{weeks:.0f} weeks){RESET}")
        else:
            print(f"    {fmt_loc(m):>5s} LOC  →  {DIM}beyond 3 years{RESET}")
    print()

# ── Fun stats ─────────────────────────────────────────────────────

total_days = CURRENT_DAY
avg_all = CURRENT_LOC / total_days
lines_per_hour = avg_all / 16

print(f"{BOLD}{'─' * 50}")
print(f"Fun Stats{RESET}")
print(f"{'─' * 50}\n")
print(f"  Average net output:    {BOLD}{avg_all:,.0f}{RESET} LOC/day")
print(f"  That's roughly:        {BOLD}{lines_per_hour:,.0f}{RESET} LOC/hour")
print(f"  Or:                    {BOLD}{lines_per_hour/60:,.1f}{RESET} LOC/minute")
print(f"  Peak single day:       {BOLD}{max(n for _,n in ACTUAL):,}{RESET} LOC (Mar 25)")
print(f"  Biggest refactor:      {BOLD}{min(n for _,n in ACTUAL):,}{RESET} LOC (Mar 26)")
print(f"  Days to write a novel: {DIM}(~80k words){RESET} {BOLD}{80000/avg_all:.1f}{RESET} days at this pace")
print()
