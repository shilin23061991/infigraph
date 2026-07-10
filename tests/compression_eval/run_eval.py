#!/usr/bin/env python3
"""Phase 6.4 eval: sweep all 4 compression levels, measure token savings
and quality retention (must_contain assertions) per level."""

import json
import subprocess
import os
import sys

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EVAL_DIR = os.path.dirname(os.path.abspath(__file__))
LEVELS = ["off", "summary", "aggressive", "minimal"]

INIT_MSG = json.dumps({
    "jsonrpc": "2.0", "id": 0, "method": "initialize",
    "params": {"protocolVersion": "2024-11-05", "capabilities": {},
               "clientInfo": {"name": "eval", "version": "1.0"}}
})
INITIALIZED_MSG = json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"})


def estimate_tokens(text):
    return int(len(text.split()) * 1.4 + 0.5)


def call_tool(binary, tool_name, args, level):
    """Call a tool at a specific compression level. Returns output text."""
    full_args = dict(args)
    full_args["path"] = PROJECT_ROOT

    req = json.dumps({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool_name, "arguments": full_args}
    })

    env = os.environ.copy()
    env["INFIGRAPH_COMPRESSION_LEVEL"] = level

    full_input = INIT_MSG + "\n" + INITIALIZED_MSG + "\n" + req + "\n"

    proc = subprocess.run(
        [binary], input=full_input, capture_output=True, text=True,
        env=env, timeout=30
    )

    for line in proc.stdout.strip().split("\n"):
        line = line.strip()
        if not line:
            continue
        try:
            resp = json.loads(line)
            if resp.get("id") == 1 and "result" in resp:
                content = resp["result"].get("content", [])
                if content:
                    return content[0].get("text", "")
        except json.JSONDecodeError:
            continue
    return ""


def check_retention(text, must_contain):
    """Check how many must_contain strings are present in output."""
    if not must_contain:
        return 1.0, [], must_contain
    found = [s for s in must_contain if s.lower() in text.lower()]
    missing = [s for s in must_contain if s.lower() not in text.lower()]
    return len(found) / len(must_contain), found, missing


def run_eval():
    with open(os.path.join(EVAL_DIR, "tasks.json")) as f:
        data = json.load(f)

    tasks = data["tasks"]

    binary = os.path.join(PROJECT_ROOT, "target", "release", "infigraph-mcp")
    if not os.path.exists(binary):
        binary = os.path.join(PROJECT_ROOT, "target", "debug", "infigraph-mcp")
    if not os.path.exists(binary):
        print("ERROR: No infigraph-mcp binary. Run cargo build --release first.")
        sys.exit(1)

    print(f"Binary: {binary}")
    print(f"Tasks: {len(tasks)}, Levels: {LEVELS}\n")

    # Results: task_id -> level -> {tokens, retention, missing}
    all_results = []

    for task in tasks:
        tid = task["id"]
        tool = task["tool"]
        must_contain = task.get("must_contain", [])
        is_sensitive = task.get("level_sensitive", True)

        print(f"  {tid} {tool:25s}", end="", flush=True)

        task_result = {"id": tid, "tool": tool, "level_sensitive": is_sensitive, "levels": {}}

        for level in LEVELS:
            try:
                text = call_tool(binary, tool, task["args"], level)
                tokens = estimate_tokens(text) if text else 0
                retention, found, missing = check_retention(text, must_contain)

                task_result["levels"][level] = {
                    "tokens": tokens,
                    "retention": retention,
                    "missing": missing,
                    "has_content": bool(text) and not text.startswith("Error:"),
                }

                symbol = "✓" if retention == 1.0 else ("◐" if retention > 0 else "✗")
                print(f"  {level[:3]}={tokens:5d}/{symbol}", end="", flush=True)

            except subprocess.TimeoutExpired:
                task_result["levels"][level] = {"tokens": 0, "retention": 0, "error": "timeout"}
                print(f"  {level[:3]}=TIMEOUT", end="", flush=True)
            except Exception as e:
                task_result["levels"][level] = {"tokens": 0, "retention": 0, "error": str(e)}
                print(f"  {level[:3]}=ERR", end="", flush=True)

        all_results.append(task_result)
        print()

    # === Summary tables ===
    print("\n" + "=" * 80)
    print("TOKEN SAVINGS BY LEVEL (level-sensitive tasks only)")
    print("=" * 80)

    sensitive_results = [r for r in all_results if r["level_sensitive"]]

    print(f"\n{'Task':6s} {'Tool':25s} {'Off':>7s} {'Summary':>7s} {'Aggr':>7s} {'Minimal':>7s} {'Sav%':>6s}")
    print("-" * 70)

    level_totals = {l: {"tokens": 0, "count": 0} for l in LEVELS}
    for r in sensitive_results:
        off_tokens = r["levels"].get("off", {}).get("tokens", 0)
        row = f"{r['id']:6s} {r['tool']:25s}"
        for level in LEVELS:
            t = r["levels"].get(level, {}).get("tokens", 0)
            level_totals[level]["tokens"] += t
            level_totals[level]["count"] += 1
            row += f" {t:7d}"
        if off_tokens > 0:
            min_tokens = r["levels"].get("minimal", {}).get("tokens", 0)
            savings = (1.0 - min_tokens / off_tokens) * 100
            row += f" {savings:5.1f}%"
        print(row)

    print("-" * 70)
    off_total = level_totals["off"]["tokens"]
    row = f"{'TOTAL':6s} {'':25s}"
    for level in LEVELS:
        t = level_totals[level]["tokens"]
        row += f" {t:7d}"
    if off_total > 0:
        min_total = level_totals["minimal"]["tokens"]
        row += f" {(1.0 - min_total / off_total) * 100:5.1f}%"
    print(row)

    # Quality retention table
    print("\n" + "=" * 80)
    print("QUALITY RETENTION BY LEVEL (must_contain check)")
    print("=" * 80)

    print(f"\n{'Task':6s} {'Tool':25s} {'Off':>5s} {'Summary':>7s} {'Aggr':>7s} {'Minimal':>7s}")
    print("-" * 65)

    level_retention = {l: {"total": 0, "sum": 0.0} for l in LEVELS}
    cliff_tasks = []

    for r in all_results:
        row = f"{r['id']:6s} {r['tool']:25s}"
        for level in LEVELS:
            ret = r["levels"].get(level, {}).get("retention", 0)
            level_retention[level]["total"] += 1
            level_retention[level]["sum"] += ret
            pct = f"{ret*100:.0f}%"
            row += f" {pct:>7s}"
        # Detect cliff: where retention first drops below 100%
        for level in LEVELS[1:]:
            ret = r["levels"].get(level, {}).get("retention", 0)
            missing = r["levels"].get(level, {}).get("missing", [])
            if ret < 1.0 and missing:
                cliff_tasks.append({
                    "task": r["id"], "tool": r["tool"],
                    "level": level, "retention": ret, "missing": missing
                })
                break
        print(row)

    print("-" * 65)
    row = f"{'AVG':6s} {'':25s}"
    for level in LEVELS:
        s = level_retention[level]
        avg = s["sum"] / s["total"] if s["total"] > 0 else 0
        row += f" {avg*100:6.1f}%"
    print(row)

    # Quality cliff report
    print("\n" + "=" * 80)
    print("QUALITY CLIFF ANALYSIS")
    print("=" * 80)

    if not cliff_tasks:
        print("\nNo quality loss detected at any level!")
    else:
        cliff_by_level = {}
        for c in cliff_tasks:
            cliff_by_level.setdefault(c["level"], []).append(c)

        for level in LEVELS:
            if level in cliff_by_level:
                tasks_at = cliff_by_level[level]
                print(f"\n  First loss at '{level}' ({len(tasks_at)} tasks):")
                for c in tasks_at:
                    print(f"    {c['task']} ({c['tool']}): {c['retention']*100:.0f}% — missing: {c['missing']}")

    # Recommendations
    print("\n" + "=" * 80)
    print("RECOMMENDATIONS")
    print("=" * 80)

    # Find safe max level (100% retention on all sensitive tasks)
    safe_level = "off"
    for level in LEVELS:
        all_good = all(
            r["levels"].get(level, {}).get("retention", 0) == 1.0
            for r in sensitive_results
            if r["levels"].get(level, {}).get("has_content", False)
        )
        if all_good:
            safe_level = level
        else:
            break

    off_tokens = level_totals["off"]["tokens"]
    safe_tokens = level_totals.get(safe_level, {}).get("tokens", off_tokens)
    savings = (1.0 - safe_tokens / off_tokens) * 100 if off_tokens > 0 else 0

    print(f"\n  Safe max level (100% retention): {safe_level}")
    print(f"  Token savings at safe level: {savings:.1f}%")
    print(f"  Recommendation: auto_level() thresholds look {'correct' if safe_level in ('summary', 'aggressive') else 'may need tuning'}")

    # Save full results
    output = {
        "version": 2,
        "phase": "6.4",
        "tasks": all_results,
        "level_totals": {l: level_totals[l] for l in LEVELS},
        "level_retention_avg": {
            l: level_retention[l]["sum"] / level_retention[l]["total"]
            if level_retention[l]["total"] > 0 else 0
            for l in LEVELS
        },
        "cliff_tasks": cliff_tasks,
        "safe_level": safe_level,
    }

    output_path = os.path.join(EVAL_DIR, "phase6_4_results.json")
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\n  Full results: {output_path}")


if __name__ == "__main__":
    run_eval()
