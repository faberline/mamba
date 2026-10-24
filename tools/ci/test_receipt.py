#!/usr/bin/env python3
"""Run one explicit libtest target, retaining inventory, exits and raw evidence.

This is an execution receipt, not a release-qualification decision. No retries,
filters, ignored-test waivers, or baseline-failure exemptions are supported.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time


def counts(inventory, output):
    names = re.findall(r"^(.+): test$", inventory, re.M)
    verdicts = re.findall(r"^test (.+?) \.\.\. (ok|FAILED|ignored)(?:,.*)?$", output, re.M)
    summaries = re.findall(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; "
        r"(\d+) ignored; (\d+) measured; (\d+) filtered out", output
    )
    observed = dict(verdicts)
    result = {key: sum(v == value for v in observed.values()) for key, value in
              (("passed", "ok"), ("failed", "FAILED"), ("ignored", "ignored"))}
    result.update(inventory=len(names), not_run=len(set(names) - observed.keys()))
    valid = (bool(names) and len(names) == len(set(names))
             and len(verdicts) == len(observed) and set(observed) == set(names)
             and len(summaries) == 1
             and tuple(map(int, summaries[0])) ==
             (result["passed"], result["failed"], result["ignored"], 0, 0)
             and result["passed"] == len(names))
    return result, valid


def run(argv, prefix, timeout):
    started = time.time()
    with prefix.with_suffix(".stdout").open("wb") as stdout, prefix.with_suffix(".stderr").open("wb") as stderr:
        proc = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True)
        timed_out = False
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
    return dict(argv=argv, pid=proc.pid, started=started, finished=time.time(),
                exit=proc.returncode, timed_out=timed_out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("target", help="lib or an exact Cargo [[test]] name")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=1800)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    selection = ["--lib"] if args.target == "lib" else ["--test", args.target]
    base = ["cargo", "test", "--locked", "-j", "1", "-p", "mamba", *selection]
    listing = run([*base, "--", "--list"], args.output / "inventory", args.timeout)
    inventory = (args.output / "inventory.stdout").read_text(errors="replace")
    names = re.findall(r"^(.+): test$", inventory, re.M)
    execution = None
    if listing["exit"] == 0 and not listing["timed_out"] and names and len(names) == len(set(names)):
        execution = run([*base, "--", "--test-threads=1"], args.output / "execution", args.timeout)
    output = (args.output / "execution.stdout").read_text(errors="replace") if execution else ""
    totals, valid = counts(inventory, output)
    passed = valid and listing["exit"] == 0 and execution is not None and execution["exit"] == 0
    passed = passed and not listing["timed_out"] and not execution["timed_out"]
    files = {p.name: dict(bytes=p.stat().st_size, sha256=hashlib.sha256(p.read_bytes()).hexdigest())
             for p in args.output.glob("*.std*")}
    receipt = dict(schema=1, target=args.target, source=os.environ.get("CI_HEAD_SHA"),
                   checkout=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                   runner=dict(os=os.environ.get("RUNNER_OS"), arch=os.environ.get("RUNNER_ARCH")),
                   inventory=listing, execution=execution, counts=totals,
                   evidence_complete=bool(passed), files=files)
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    message = f"{args.target}: {json.dumps(totals)}; evidence_complete={bool(passed)}"
    print(message, flush=True)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as stream:
            stream.write(message + "\n\n")
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
