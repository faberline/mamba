use super::*;

pub(super) fn cmd_bench(sub: &ArgMatches) -> Result<()> {
    if sub.contains_id("fixtures")
        && sub.value_source("fixtures") != Some(clap::parser::ValueSource::DefaultValue)
    {
        let dir = sub
            .get_one::<String>("fixtures")
            .map(|s| std::path::PathBuf::from(s))
            .unwrap_or_else(|| {
                std::path::PathBuf::from(format!(
                    "{}/core/bench",
                    mamba::conformance::FIXTURES_ROOT
                ))
            });
        let mamba_bin =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("mamba"));
        let fixtures = mamba::bench::discover_fixtures(&dir);
        if fixtures.is_empty() {
            anyhow::bail!("no .py fixtures found in {}", dir.display());
        }
        println!(
            "Running {} fixture benchmarks from {}...",
            fixtures.len(),
            dir.display()
        );
        let rows = mamba::bench::run_fixture_suite(&fixtures, &mamba_bin);
        mamba::bench::print_fixture_report(&rows);
        let ok = rows
            .iter()
            .filter(|r| r.mamba.as_ref().map_or(false, |m| m.correct))
            .count();
        let fail = rows.len() - ok;
        println!("{ok} correct, {fail} failed.");
        return Ok(());
    }

    let compare = sub.get_one::<String>("compare").map(|s| s.as_str());
    let filter = sub.get_one::<String>("filter").map(|s| s.as_str());
    let file = sub.get_one::<String>("file");

    let runner = if matches!(compare, Some("cpython")) {
        BenchRunner::default()
    } else {
        BenchRunner::mamba_only()
    };

    let suite = if let Some(path) = file {
        let source =
            std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read {path}: {e}"))?;
        let leaked: &'static str = Box::leak(source.into_boxed_str());
        let name: &'static str = Box::leak(path.to_string().into_boxed_str());
        BenchSuite {
            benchmarks: vec![mamba::bench::Benchmark {
                name,
                source: leaked,
                kind: mamba::bench::BenchKind::Workload,
                iters: 10,
            }],
        }
    } else {
        BenchSuite::builtin()
    };

    let filtered_suite = if let Some(kind_str) = filter {
        use mamba::bench::BenchKind;
        let kind = match kind_str {
            "numeric" => BenchKind::Numeric,
            "recursion" => BenchKind::Recursion,
            "workload" => BenchKind::Workload,
            other => anyhow::bail!("unknown kind {other:?}; use: numeric, recursion, workload"),
        };
        BenchSuite {
            benchmarks: suite
                .benchmarks
                .into_iter()
                .filter(|b| b.kind == kind)
                .collect(),
        }
    } else {
        suite
    };

    println!(
        "Running Mamba benchmark suite ({} benchmarks)...",
        filtered_suite.benchmarks.len()
    );
    let rows = run_suite(&filtered_suite, &runner);
    print_report(&rows);
    let ok = rows.iter().filter(|r| r.mamba_ns_mean.is_some()).count();
    let err = rows.len() - ok;
    println!("{ok} succeeded, {err} failed.");

    // --json <path>: emit a baseline-friendly JSON snapshot of the run.
    if let Some(path) = sub.get_one::<String>("json") {
        write_baseline_json(path, &rows)
            .map_err(|e| anyhow::anyhow!("write baseline {path}: {e}"))?;
        println!("baseline written to {path}");
    }

    // --check <baseline>: regression gate. Compares mamba_ns_mean against the
    // committed baseline; exits non-zero if any benchmark is more than 10%
    // slower than the baseline median (CI hook).
    if let Some(path) = sub.get_one::<String>("check") {
        let regressions = check_against_baseline(path, &rows, 1.10)
            .map_err(|e| anyhow::anyhow!("read baseline {path}: {e}"))?;
        if !regressions.is_empty() {
            for line in &regressions {
                eprintln!("regression: {line}");
            }
            anyhow::bail!("{} benchmark regression(s) vs {}", regressions.len(), path);
        }
        println!("no regression vs {path}");
    }

    Ok(())
}

/// JSON writer for `mamba bench --json baseline.json`.
///
/// Format (kept intentionally simple so a future CI gate can diff easily):
/// ```json
/// {
///   "version": 1,
///   "benchmarks": [
///     {"name": "fib30", "kind": "Recursion",
///      "mamba_ns": 12345, "cpython_ns": 67890,
///      "speedup_vs_cpython": 5.5}
///   ]
/// }
/// ```
fn write_baseline_json(path: &str, rows: &[mamba::bench::ReportRow]) -> std::io::Result<()> {
    use std::fmt::Write as _;
    let mut s = String::from("{\n  \"version\": 1,\n  \"benchmarks\": [");
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("\n    {");
        let _ = write!(s, "\"name\": {:?}, ", r.name);
        let _ = write!(s, "\"kind\": {:?}", format!("{:?}", r.kind));
        if let Some(v) = r.mamba_ns_mean {
            let _ = write!(s, ", \"mamba_ns\": {v}");
        }
        if let Some(v) = r.cpython_ns_mean {
            let _ = write!(s, ", \"cpython_ns\": {v}");
        }
        if let Some(v) = r.pypy_ns_mean {
            let _ = write!(s, ", \"pypy_ns\": {v}");
        }
        if let Some(sp) = r.speedup() {
            let _ = write!(s, ", \"speedup_vs_cpython\": {sp:.4}");
        }
        if let Some(err) = r.mamba_error.as_deref() {
            let _ = write!(s, ", \"error\": {err:?}");
        }
        s.push('}');
    }
    s.push_str("\n  ]\n}\n");
    std::fs::write(path, s)
}

/// Compare current results against a committed baseline JSON. Returns a list
/// of human-readable regression descriptions; an empty list means no
/// benchmark exceeded the regression threshold.
///
/// `threshold` is the multiplier on baseline mean time before a slower run
/// counts as a regression (e.g. 1.10 = "fail if 10% slower than baseline").
fn check_against_baseline(
    path: &str,
    rows: &[mamba::bench::ReportRow],
    threshold: f64,
) -> std::io::Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)?;
    // Tiny ad-hoc parser to avoid pulling in serde_json: extract
    // "name" → mamba_ns pairs from the simple shape we emit above.
    let mut baseline: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for chunk in raw.split('{').skip(2) {
        let name = match chunk.find("\"name\":") {
            Some(start) => {
                let after = &chunk[start + 7..];
                let q = match after.find('"') {
                    Some(q) => q,
                    None => continue,
                };
                let after_q = &after[q + 1..];
                let end = match after_q.find('"') {
                    Some(e) => e,
                    None => continue,
                };
                after_q[..end].to_string()
            }
            None => continue,
        };
        let mamba_ns = match chunk.find("\"mamba_ns\":") {
            Some(start) => {
                let after = chunk[start + 11..].trim_start();
                let end = after
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(after.len());
                after[..end].parse::<u64>().ok()
            }
            None => None,
        };
        if let Some(ns) = mamba_ns {
            baseline.insert(name, ns);
        }
    }

    let mut out = Vec::new();
    for r in rows {
        let now = match r.mamba_ns_mean {
            Some(v) => v,
            None => continue,
        };
        if let Some(&base) = baseline.get(&r.name) {
            let ratio = now as f64 / base.max(1) as f64;
            if ratio > threshold {
                out.push(format!(
                    "{} now {} ns/op vs baseline {} ns/op ({:.2}× slower)",
                    r.name, now, base, ratio
                ));
            }
        }
    }
    Ok(out)
}
