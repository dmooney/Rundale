#!/usr/bin/env python3
"""
Rundale Project Stats
=====================
Comprehensive project health dashboard: LOC by language, commit
velocity, contributor breakdown, LOC growth projections, and fun stats.

Reads everything live from git history. Code-only (excludes .md).
"""

import math
import subprocess
from datetime import date, timedelta

CODE_EXTENSIONS = "*.rs *.ts *.svelte *.js *.json *.toml *.css *.html *.sh *.py *.txt"

BOLD = "\033[1m"
DIM = "\033[2m"
GREEN = "\033[32m"
CYAN = "\033[36m"
YELLOW = "\033[33m"
MAGENTA = "\033[35m"
RED = "\033[31m"
RESET = "\033[0m"
BAR_CHAR = "█"
BAR_HALF = "▌"


def fmt_loc(n):
    if n >= 1_000_000:
        return f"{n/1_000_000:.1f}M"
    if n >= 1_000:
        return f"{n/1_000:.1f}k"
    return str(n)


def bar(value, max_val, width=30):
    if max_val == 0:
        return " " * width
    filled = value / max_val * width
    full = int(filled)
    half = 1 if filled - full >= 0.5 else 0
    return BAR_CHAR * full + (BAR_HALF if half else "") + " " * (width - full - half)


# ── Data collection ─────────────────────────────────────────────

def git_daily_loc():
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


def git_commits_since(days_ago):
    result = subprocess.run(
        ["git", "log", "--all", "--oneline", f"--since={days_ago} days ago"],
        capture_output=True, text=True,
    )
    return len(result.stdout.strip().splitlines())


def git_loc_since(days_ago):
    exts = CODE_EXTENSIONS.split()
    cmd = ["git", "log", "--all", "--format=%ad", "--date=short", "--numstat",
           f"--since={days_ago} days ago", "--"] + exts
    result = subprocess.run(cmd, capture_output=True, text=True)
    added = 0
    deleted = 0
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[0] != '-':
            try:
                added += int(parts[0])
                deleted += int(parts[1])
            except ValueError:
                pass
    return added, deleted


def git_contributors():
    result = subprocess.run(
        ["git", "shortlog", "-sn", "--all"], capture_output=True, text=True
    )
    contribs = []
    for line in result.stdout.strip().splitlines():
        parts = line.strip().split("\t", 1)
        if len(parts) == 2:
            contribs.append((parts[1].strip(), int(parts[0].strip())))
    return contribs


def loc_by_extension_on_disk():
    exts = CODE_EXTENSIONS.split()
    exclude = [".git", "node_modules", "target", "dist", "build"]
    totals = {}
    file_counts = {}
    for ext_glob in exts:
        ext = ext_glob.lstrip("*")
        find_cmd = ["find", ".", "-type", "f", "-name", ext_glob]
        for d in exclude:
            find_cmd = find_cmd[:2] + ["-path", f"./{d}", "-prune", "-o"] + find_cmd[2:]
        find_cmd.append("-print")
        found = subprocess.run(find_cmd, capture_output=True, text=True)
        files = [f for f in found.stdout.strip().splitlines() if f]
        if not files:
            continue
        wc = subprocess.run(["wc", "-l"] + files, capture_output=True, text=True)
        lines = wc.stdout.strip().splitlines()
        if len(files) == 1:
            count = int(lines[0].split()[0]) if lines else 0
        else:
            total_line = [l for l in lines if "total" in l]
            count = int(total_line[-1].split()[0]) if total_line else 0
        if count > 0:
            totals[ext] = count
            file_counts[ext] = len(files)
    return totals, file_counts


def git_busiest_days(top_n=5):
    result = subprocess.run(
        ["git", "log", "--all", "--format=%ad", "--date=short"],
        capture_output=True, text=True,
    )
    counts = {}
    for line in result.stdout.strip().splitlines():
        counts[line] = counts.get(line, 0) + 1
    ranked = sorted(counts.items(), key=lambda x: -x[1])
    return ranked[:top_n]


# ── Collect data ────────────────────────────────────────────────

ACTUAL = git_daily_loc()
START_DATE = date.fromisoformat(ACTUAL[0][0]) if ACTUAL else date.today()
CURRENT_LOC = sum(net for _, net in ACTUAL)
CURRENT_DAY = (date.fromisoformat(ACTUAL[-1][0]) - START_DATE).days if ACTUAL else 0
COMMIT_COUNT = git_commit_count()
CONTRIBUTORS = git_contributors()
LOC_BY_EXT, FILE_COUNTS = loc_by_extension_on_disk()
BUSIEST = git_busiest_days()

commits_7d = git_commits_since(7)
commits_14d = git_commits_since(14)
added_7d, deleted_7d = git_loc_since(7)
added_14d, deleted_14d = git_loc_since(14)

elapsed = CURRENT_DAY + 1

# ── Header ──────────────────────────────────────────────────────

print(f"\n{BOLD}Rundale Project Dashboard{RESET}  {DIM}{date.today().isoformat()}{RESET}")
print(f"{'=' * 60}")

# ── Overview ────────────────────────────────────────────────────

print(f"\n{BOLD}{CYAN}Overview{RESET}")
print(f"{'─' * 60}")
print(f"  Total LOC (code only):  {BOLD}{CURRENT_LOC:>10,}{RESET}")
print(f"  Total commits:          {BOLD}{COMMIT_COUNT:>10,}{RESET}")
print(f"  Project age:            {BOLD}{elapsed:>10} days{RESET}  {DIM}(since {START_DATE}){RESET}")
print(f"  Contributors:           {BOLD}{len(CONTRIBUTORS):>10}{RESET}")
if elapsed > 0:
    print(f"  Avg commits/day:        {BOLD}{COMMIT_COUNT/elapsed:>10.1f}{RESET}")
    print(f"  Avg LOC/day:            {BOLD}{CURRENT_LOC/elapsed:>10,.0f}{RESET}")

# ── Recent velocity ─────────────────────────────────────────────

print(f"\n{BOLD}{CYAN}Recent Velocity{RESET}")
print(f"{'─' * 60}")
net_7d = added_7d - deleted_7d
net_14d = added_14d - deleted_14d
print(f"  Last  7 days:  {GREEN}+{added_7d:>8,}{RESET} / {RED}-{deleted_7d:>8,}{RESET}  = net {BOLD}{net_7d:>+9,}{RESET}  ({commits_7d} commits)")
print(f"  Last 14 days:  {GREEN}+{added_14d:>8,}{RESET} / {RED}-{deleted_14d:>8,}{RESET}  = net {BOLD}{net_14d:>+9,}{RESET}  ({commits_14d} commits)")

# ── LOC by language ─────────────────────────────────────────────

disk_total = sum(LOC_BY_EXT.values())
print(f"\n{BOLD}{CYAN}LOC by Language{RESET}  {DIM}(on disk: {disk_total:,} LOC, {sum(FILE_COUNTS.values())} files){RESET}")
print(f"{'─' * 60}")
sorted_exts = sorted(LOC_BY_EXT.items(), key=lambda x: -x[1])
max_ext_loc = max(LOC_BY_EXT.values()) if LOC_BY_EXT else 1
ext_labels = {
    ".rs": "Rust", ".ts": "TypeScript", ".svelte": "Svelte", ".js": "JavaScript",
    ".json": "JSON", ".toml": "TOML", ".css": "CSS", ".html": "HTML",
    ".sh": "Shell", ".py": "Python", ".txt": "Text",
}
for ext, loc in sorted_exts:
    label = ext_labels.get(ext, ext)
    b = bar(loc, max_ext_loc, 20)
    fc = FILE_COUNTS.get(ext, 0)
    pct = loc / disk_total * 100 if disk_total else 0
    print(f"  {label:<12s}  {GREEN}{b}{RESET}  {loc:>8,}  {DIM}{fc:>3} files{RESET}  ({pct:4.1f}%)")

# ── Contributors ────────────────────────────────────────────────

print(f"\n{BOLD}{CYAN}Contributors{RESET}")
print(f"{'─' * 60}")
max_contrib = CONTRIBUTORS[0][1] if CONTRIBUTORS else 1
for name, count in CONTRIBUTORS:
    b = bar(count, max_contrib, 20)
    pct = count / COMMIT_COUNT * 100 if COMMIT_COUNT else 0
    print(f"  {name:<25s}  {CYAN}{b}{RESET}  {count:>4} commits ({pct:4.1f}%)")

# ── Busiest days ────────────────────────────────────────────────

print(f"\n{BOLD}{CYAN}Busiest Days (by commits){RESET}")
print(f"{'─' * 60}")
max_busy = BUSIEST[0][1] if BUSIEST else 1
for d, c in BUSIEST:
    b = bar(c, max_busy, 20)
    print(f"  {d}  {YELLOW}{b}{RESET}  {c} commits")

# ── Daily LOC chart ─────────────────────────────────────────────

print(f"\n{BOLD}{CYAN}Daily Net LOC{RESET}")
print(f"{'─' * 60}")

max_abs = max(abs(d[1]) for d in ACTUAL) if ACTUAL else 1
cumulative = 0
for datestr, net in ACTUAL:
    cumulative += net
    day_num = (date.fromisoformat(datestr) - START_DATE).days
    if net >= 0:
        b = bar(net, max_abs, 25)
        print(f"  Day {day_num:2d} {DIM}{datestr}{RESET}  {GREEN}{b}{RESET} {net:>+8,}  ({fmt_loc(cumulative)})")
    else:
        b = bar(abs(net), max_abs, 25)
        print(f"  Day {day_num:2d} {DIM}{datestr}{RESET}  {MAGENTA}{b}{RESET} {net:>+8,}  ({fmt_loc(cumulative)})")

# ── Projections ─────────────────────────────────────────────────

MILESTONES = [100_000, 250_000, 500_000, 1_000_000]

positive_days = [n for _, n in ACTUAL if n > 0]
recent = positive_days[-7:]
avg_recent = sum(recent) / len(recent) if recent else 5000


def project(daily_fn):
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
    ("Ramp + decay", "peak ~day 30, gradual slowdown", scenario_ramp),
    ("Hypergrowth", "AI-assisted, sustained high output", scenario_hyper),
    ("Early plateau", "rapid decay to maintenance mode", scenario_mature),
]

print(f"\n{BOLD}{CYAN}LOC Milestone Projections{RESET}")
print(f"{'─' * 60}")

for name, desc, fn in scenarios:
    results = project(fn)
    print(f"\n  {YELLOW}{BOLD}{name}{RESET} {DIM}({desc}){RESET}")
    for m in MILESTONES:
        if m in results:
            day, dt = results[m]
            remaining = day - CURRENT_DAY
            if remaining <= 0:
                eta = "already reached"
            elif remaining < 7:
                eta = f"in {remaining}d"
            else:
                eta = f"in ~{remaining // 7} weeks"
            print(f"    {fmt_loc(m):>5s} LOC  →  {CYAN}{dt.strftime('%b %d, %Y')}{RESET}"
                  f"  {DIM}({eta}){RESET}")
        else:
            print(f"    {fmt_loc(m):>5s} LOC  →  {DIM}beyond 3 years{RESET}")

# ── Fun stats ───────────────────────────────────────────────────

avg_all = CURRENT_LOC / elapsed if elapsed else 0
lines_per_hour = avg_all / 16

print(f"\n{BOLD}{CYAN}Fun Stats{RESET}")
print(f"{'─' * 60}")
print(f"  Average net output:    {BOLD}{avg_all:,.0f}{RESET} LOC/day")
print(f"  That's roughly:        {BOLD}{lines_per_hour:,.0f}{RESET} LOC/hour  {DIM}(16h workday){RESET}")
print(f"  Or:                    {BOLD}{lines_per_hour/60:,.1f}{RESET} LOC/minute")
if ACTUAL:
    peak_day = max(ACTUAL, key=lambda x: x[1])
    min_day = min(ACTUAL, key=lambda x: x[1])
    print(f"  Peak single day:       {BOLD}+{peak_day[1]:,}{RESET} LOC ({peak_day[0]})")
    print(f"  Biggest refactor:      {BOLD}{min_day[1]:,}{RESET} LOC ({min_day[0]})")
if avg_all > 0:
    print(f"  Days to write a novel: {DIM}(~80k words){RESET} {BOLD}{80000/avg_all:.1f}{RESET} days at this pace")
print()
