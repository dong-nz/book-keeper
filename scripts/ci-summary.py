# NFR-10 (SPEC-v0.0.11 §3.4, AC-158): the combined wall-time summary job's
# report. The workflow pipes the run's own job records (`gh api …/jobs`) into
# this script on stdin; it prints the per-job walls and the combined span to
# stdout, which the workflow appends to the run summary. The measurement is
# D29's data source; the 300 s gate is the local contract (DR-357), not this
# script's call. Extracted from the inline form the first YAML validation
# rejected (a column-0 script body breaks a block scalar) — same behavior,
# now testable on a synthetic payload (see the spec's CI design).
import json
import sys
from datetime import datetime

jobs = json.load(sys.stdin)


def parse(s):
    return datetime.fromisoformat(s) if s else None


rows = []
starts, ends = [], []
for job in jobs:
    if job["name"] == "summary":
        continue
    started, ended = parse(job.get("started_at")), parse(job.get("completed_at"))
    if started and ended:
        rows.append((job["name"], (ended - started).total_seconds()))
        starts.append(started)
        ends.append(ended)

print("## Run wall-time summary (AC-158 / D29)")
print()
print("| Job | Wall |")
print("|---|---|")
for name, seconds in rows:
    print(f"| {name} | {int(seconds)} s |")
if starts and ends:
    combined = (max(ends) - min(starts)).total_seconds()
    print()
    print(f"Combined span: {int(combined)} s (budget 300000 ms is the local contract, "
          "DR-357 — the CI lane job reports its measured wall in its own summary).")
