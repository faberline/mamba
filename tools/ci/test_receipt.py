#!/usr/bin/env python3
"""Run one explicit libtest target, retaining inventory, exits and raw evidence.

This is an execution receipt, not a release-qualification decision. Exact-case
receipts have an explicitly limited scope; unfiltered receipts still reject
filtering. No retries, ignored-test waivers or baseline exemptions are supported.
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


def source_identity_matches(expected, checked_out):
    return bool(expected and expected == checked_out
                and re.fullmatch(r"[0-9a-f]{40}", checked_out))


def inventory_names(inventory, required=()):
    names = re.findall(r"^(.+): test$", inventory, re.M)
    totals = re.findall(r"^(\d+) tests?, (\d+) benchmarks?$", inventory, re.M)
    complete = (bool(names) and len(names) == len(set(names))
                and len(totals) == 1 and tuple(map(int, totals[0])) == (len(names), 0)
                and set(required).issubset(names))
    return names, complete


def exact_inventory_matches(inventory, case):
    names, complete = inventory_names(inventory, [case])
    return bool(case and complete and names == [case])


def counts(inventory, output, expected_filtered=0):
    names, complete_inventory = inventory_names(inventory)
    verdicts = re.findall(
        r"^test (.+?)(?: - should panic)? \.\.\. (ok|FAILED|ignored)(?:,.*)?$", output, re.M
    )
    summaries = re.findall(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; "
        r"(\d+) ignored; (\d+) measured; (\d+) filtered out", output
    )
    observed = dict(verdicts)
    result = {key: sum(v == value for v in observed.values()) for key, value in
              (("passed", "ok"), ("failed", "FAILED"), ("ignored", "ignored"))}
    result.update(inventory=len(names), inventory_complete=complete_inventory,
                  not_run=len(set(names) - observed.keys()),
                  filtered_out=int(summaries[0][4]) if len(summaries) == 1 else None)
    valid = (complete_inventory
             and len(verdicts) == len(observed) and set(observed) == set(names)
             and len(summaries) == 1
             and tuple(map(int, summaries[0])) ==
             (result["passed"], result["failed"], result["ignored"], 0, expected_filtered)
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
    parser.add_argument("--inventory-timeout", type=int,
                        help="Separate deadline for the listing command, which includes compilation")
    parser.add_argument("--require-case", action="append", default=[],
                        help="Require this exact inventory name without filtering execution")
    parser.add_argument("--exact-case",
                        help="Run only this exact case; receipt never qualifies the full target")
    args = parser.parse_args()
    if args.exact_case is not None and not args.exact_case:
        parser.error("--exact-case must name a nonempty case")
    args.output.mkdir(parents=True, exist_ok=True)
    source = os.environ.get("CI_HEAD_SHA")
    checkout_before = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    identity_before = source_identity_matches(source, checkout_before)
    selection = ["--lib"] if args.target == "lib" else ["--test", args.target]
    base = ["cargo", "test", "--locked", "-j", "1", "-p", "mamba", *selection]
    required = args.require_case + ([args.exact_case] if args.exact_case else [])
    catalogue = None
    catalogue_names = []
    catalogue_ok = identity_before
    expected_filtered = 0
    filters = [args.exact_case, "--exact"] if args.exact_case else []
    if args.exact_case and identity_before:
        catalogue = run([*base, "--", "--list"], args.output / "catalogue",
                        args.inventory_timeout or args.timeout)
        catalogue_names, catalogue_complete = inventory_names(
            (args.output / "catalogue.stdout").read_text(errors="replace"), required)
        catalogue_ok = catalogue["exit"] == 0 and not catalogue["timed_out"] and catalogue_complete
        expected_filtered = max(len(catalogue_names) - 1, 0)
    listing = None
    inventory = ""
    if catalogue_ok:
        listing = run([*base, "--", *filters, "--list"], args.output / "inventory",
                      args.inventory_timeout or args.timeout)
        inventory = (args.output / "inventory.stdout").read_text(errors="replace")
    names, complete_inventory = inventory_names(inventory, required)
    if args.exact_case:
        complete_inventory = complete_inventory and exact_inventory_matches(inventory, args.exact_case)
    missing_required_cases = sorted(set(required) - set(names))
    execution = None
    if listing and listing["exit"] == 0 and not listing["timed_out"] and complete_inventory:
        execution = run([*base, "--", *filters, "--test-threads=1"],
                        args.output / "execution", args.timeout)
    output = (args.output / "execution.stdout").read_text(errors="replace") if execution else ""
    totals, valid = counts(inventory, output, expected_filtered=expected_filtered)
    passed = (valid and catalogue_ok and complete_inventory and listing is not None
              and listing["exit"] == 0 and execution is not None and execution["exit"] == 0)
    passed = passed and not listing["timed_out"] and not execution["timed_out"]
    checkout = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    identity_valid = identity_before and source_identity_matches(source, checkout)
    passed = passed and identity_valid
    files = {p.name: dict(bytes=p.stat().st_size, sha256=hashlib.sha256(p.read_bytes()).hexdigest())
             for p in args.output.glob("*.std*")}
    receipt = dict(schema=1, target=args.target, source=source,
                   checkout_before=checkout_before, checkout=checkout, source_identity_valid=identity_valid,
                   selection_scope="single_exact_case" if args.exact_case else "complete_target",
                   exact_case=args.exact_case, qualifies_complete_target=bool(passed and not args.exact_case),
                   catalogue=catalogue, catalogue_count=len(catalogue_names) if args.exact_case else None,
                   expected_filtered_out=expected_filtered,
                   required_cases=required, missing_required_cases=missing_required_cases,
                   runner=dict(os=os.environ.get("RUNNER_OS"), arch=os.environ.get("RUNNER_ARCH")),
                   inventory=listing, execution=execution, counts=totals,
                   evidence_complete=bool(passed), files=files)
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    message = f"{args.target}/{args.exact_case or 'unfiltered'}: {json.dumps(totals)}; evidence_complete={bool(passed)}"
    print(message, flush=True)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as stream:
            stream.write(message + "\n\n")
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
