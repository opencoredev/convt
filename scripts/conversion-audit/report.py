"""Prints the Markdown tables for docs/conversion-audit-*.md from audit.py's JSON.

    python3 -I scripts/conversion-audit/report.py REPORT.json
"""

import json
import sys
from collections import defaultdict

FAMILIES = ["image", "vector", "video", "audio", "pdf", "document", "presentation", "spreadsheet"]


def main():
    data = json.load(open(sys.argv[1]))
    results = data["results"]
    menus = data["targets"]

    tally = defaultdict(lambda: defaultdict(int))
    for r in results:
        tally[r["family"]][r["result"]] += 1
    print("| Input family | Pairs | Pass | Fail | Skipped |")
    print("|---|---:|---:|---:|---:|")
    total = defaultdict(int)
    for fam in FAMILIES:
        t = tally[fam]
        n = sum(t.values())
        for k in ("pass", "fail", "skipped"):
            total[k] += t[k]
        print(f"| {fam} | {n} | {t['pass']} | {t['fail']} | {t['skipped']} |")
    print(f"| **all** | **{sum(total.values())}** | **{total['pass']}** | **{total['fail']}** | **{total['skipped']}** |")
    print()

    by_from = defaultdict(list)
    for r in results:
        by_from[r["from"]].append(r)
    print("| Input | Family | Targets offered | Passed | Failed | Right-click menu |")
    print("|---|---|---:|---|---|---|")
    for fid, menu in menus.items():
        rows = by_from[fid]
        passed = [r["to"] for r in rows if r["result"] == "pass"]
        failed = [r["to"] for r in rows if r["result"] == "fail"]
        fam = rows[0]["family"] if rows else "?"
        print(
            f"| {fid} | {fam} | {len(menu['targets'])} | {' '.join(passed) or '-'} | "
            f"{' '.join(failed) or '-'} | {' '.join(menu['menu'])} |"
        )
    print()

    bad_menu = {
        fid: [m for m in menu["menu"] if m not in menu["targets"]]
        for fid, menu in menus.items()
    }
    bad_menu = {k: v for k, v in bad_menu.items() if v}
    empty = [fid for fid, menu in menus.items() if not menu["menu"]]
    print(f"Menu entries not in `convt targets`: {bad_menu or 'none'}. Inputs with an empty menu: {empty or 'none'}.")
    print()

    print("<details><summary>Every pair, with the evidence the checker recorded</summary>")
    print()
    print("| From | To | Result | Evidence |")
    print("|---|---|---|---|")
    for r in sorted(results, key=lambda r: (FAMILIES.index(r["family"]), r["from"], r["to"])):
        detail = r["detail"].replace("|", "\\|").replace("\n", " ")
        print(f"| {r['from']} | {r['to']} | {r['result']} | {detail} |")
    print()
    print("</details>")


if __name__ == "__main__":
    main()
