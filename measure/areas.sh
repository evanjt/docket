#!/bin/sh
# Where each item's one area would come from if every concept item became an area: its nearest plan
# up the opened links that is tied to a concept, else a concept tied to the item itself. Prints, per
# project with any items, the concept count and how many items (open, all) get an area from their
# plan, from their own tie, from several ties at once, or from nothing. Reads the docket server
# through the client, so it needs a bound client and changes nothing.
set -eu
docket --json projects | python3 -c '
import json, subprocess, sys
for p in json.load(sys.stdin):
    slug = p["slug"]
    g = json.loads(subprocess.run(["docket", "-p", slug, "--json", "graph", "--no-files"],
                                  capture_output=True, text=True, check=True).stdout)
    nodes = {n["id"]: n for n in g["nodes"]}
    if not nodes:
        continue
    concept = {i for i, n in nodes.items() if n["kind"] == "concept"}
    standing = concept | {i for i, n in nodes.items() if n["kind"] == "idea"}
    tied, up = {}, {}
    for e in g["edges"]:
        a, b, k = e["from"], e["to"], e["kind"]
        if k == "opened" and nodes.get(b, {}).get("kind") in ("audit", "story", "package"):
            up.setdefault(a, b)
        if k in ("related", "opened") and (a in concept) != (b in concept):
            item, con = (b, a) if a in concept else (a, b)
            tied.setdefault(item, set()).add(con)
    def plan_area(i):
        seen, at = {i}, up.get(i)
        while at and at not in seen:
            seen.add(at)
            if len(tied.get(at, ())) == 1:
                return True
            at = up.get(at)
        return False
    counts = {"plan": [0, 0], "own": [0, 0], "several": [0, 0], "none": [0, 0]}
    for i, n in nodes.items():
        if i in standing:
            continue
        own = tied.get(i, set())
        if n["kind"] in ("audit", "story", "package") and len(own) == 1:
            src = "own"
        elif plan_area(i):
            src = "plan"
        elif len(own) == 1:
            src = "own"
        elif own:
            src = "several"
        else:
            src = "none"
        counts[src][1] += 1
        counts[src][0] += n["state"] == "open"
    print(slug, "concepts", len(concept),
          " ".join(f"{k} {o}/{a}" for k, (o, a) in counts.items()))
'
