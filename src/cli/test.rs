use super::*;

/// Run ONE fixture in the current (forked child) process and return an exit
/// code: 0 = PASS, 1 = FAIL. The child captures its own stdout to a temp file,
/// runs the full compile+execute pipeline under `catch_unwind`, and decides
/// PASS = (run returned Ok) AND stdout contains "<stem> OK" (the fixture
/// self-check convention). A hard crash (SIGSEGV/abort/stack-overflow) never
/// reaches here — the kernel kills the child and the parent records CRASH.
fn run_one_fixture(path: &str) -> i32 {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // Redirect stdout -> per-child temp file, stderr -> /dev/null, so the
    // fixture's prints are captured and compiler diagnostics don't pollute the
    // parent's terminal. Use the raw fds (we never return; _exit follows).
    let pid = unsafe { libc::getpid() };
    let tmp = format!("/tmp/mb_testbatch_{pid}.out");
    unsafe {
        if let Ok(c_tmp) = std::ffi::CString::new(tmp.as_str()) {
            let fd = libc::open(
                c_tmp.as_ptr(),
                libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC,
                0o600,
            );
            if fd >= 0 {
                libc::dup2(fd, 1);
                libc::close(fd);
            }
        }
        if let Ok(devnull) = std::ffi::CString::new("/dev/null") {
            let nfd = libc::open(devnull.as_ptr(), libc::O_WRONLY);
            if nfd >= 0 {
                libc::dup2(nfd, 2);
                libc::close(nfd);
            }
        }
    }

    let owned = path.to_string();
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let config = CompilerConfig {
            backend: Backend::CraneliftJit,
            project_config: None,
            ..Default::default()
        };
        let mut session = CompilerSession::new(config);
        session.run(&owned)
    }));

    // Flush the captured stdout then read it back to check the OK marker.
    use std::io::Write as _;
    std::io::stdout().flush().ok();
    let captured = std::fs::read_to_string(&tmp).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);

    let ok = matches!(ran, Ok(Ok(()))) && captured.contains(&format!("{stem} OK"));
    if ok {
        0
    } else {
        1
    }
}

/// Zygote-fork batch conformance runner. The fixed ~0.16s per-process init
/// (Cranelift ISA + eager stdlib registration) is the dominant cost of a
/// process-per-fixture sweep; here we pay it ONCE in this (parent) process,
/// then `fork()` a worker per fixture. Each child COW-inherits the warm
/// runtime (skipping stdlib re-registration) and runs in an isolated address
/// space — so per-fixture isolation is sound (no cross-fixture state leakage)
/// and a crashing fixture only kills its own child, never the pool.
pub(super) fn cmd_test_batch(sub: &ArgMatches) -> Result<()> {
    use std::collections::HashMap;
    use std::io::{Read as _, Write as _};
    use std::time::Instant;

    let paths: Vec<String> = {
        let text = if let Some(pf) = sub.get_one::<String>("paths") {
            std::fs::read_to_string(pf).with_context(|| format!("reading {pf}"))?
        } else {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .context("reading stdin")?;
            s
        };
        text.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };
    let jobs: usize = sub
        .get_one::<String>("jobs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(8)
        .max(1);
    let timeout_secs: u64 = sub
        .get_one::<String>("timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let total = paths.len();
    if total == 0 {
        println!("test-batch: 0 fixtures");
        return Ok(());
    }

    // WARM the zygote: register stdlib + search paths into THIS process's
    // thread-locals so forked children inherit them (COW) and skip the
    // ~40-60ms re-registration. Critically, we do NOT compile/run any fixture
    // in the parent — so JIT_LOCK is never held and no user/async state exists
    // at any fork point, keeping the children's inherited locks clean.
    mamba::runtime::module::mb_register_native_modules();
    mamba::runtime::module::mb_register_builtins();
    mamba::runtime::module::mb_init_search_paths();
    // Build the Cranelift ISA + runtime symbol-table caches in the PARENT so
    // every forked worker COW-inherits them read-only instead of each rebuilding
    // them (fork-per-fixture = one compile per child = zero LazyLock amortization
    // unless the caches are warm before the fork).
    mamba::codegen::cranelift::jit::warm_jit_caches();

    // Flush parent buffers before forking so nothing is duplicated in children.
    std::io::stdout().flush().ok();
    std::io::stderr().flush().ok();

    let start = Instant::now();
    // verdict: 1=pass 2=fail 3=timeout 4=crash
    let mut results: Vec<u8> = vec![0; total];
    let mut inflight: HashMap<i32, (usize, Instant)> = HashMap::new();
    let mut next = 0usize;
    let (mut pass, mut fail, mut timeout_n, mut crash) = (0usize, 0usize, 0usize, 0usize);

    loop {
        // Fill the pool.
        while inflight.len() < jobs && next < total {
            let idx = next;
            next += 1;
            let path = paths[idx].clone();
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                // CHILD — run one fixture, never return.
                let code = run_one_fixture(&path);
                unsafe { libc::_exit(code) };
            } else if pid > 0 {
                inflight.insert(pid, (idx, Instant::now()));
            } else {
                // fork() failed — treat as crash and keep going.
                results[idx] = 4;
                crash += 1;
            }
        }
        if inflight.is_empty() && next >= total {
            break;
        }

        // Reap one finished child without blocking; otherwise check timeouts.
        let mut status: libc::c_int = 0;
        let r = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if r > 0 {
            if let Some((idx, _)) = inflight.remove(&r) {
                let exited = libc::WIFEXITED(status);
                let code = if exited {
                    libc::WEXITSTATUS(status)
                } else {
                    -1
                };
                match (exited, code) {
                    (true, 0) => {
                        results[idx] = 1;
                        pass += 1;
                    }
                    (true, 1) => {
                        results[idx] = 2;
                        fail += 1;
                    }
                    _ => {
                        // killed by signal / abnormal exit = hard crash
                        results[idx] = 4;
                        crash += 1;
                    }
                }
            }
        } else {
            // Nothing ready: enforce timeouts, then yield briefly.
            let now = Instant::now();
            let killed: Vec<(i32, usize)> = inflight
                .iter()
                .filter(|(_, (_, st))| now.duration_since(*st).as_secs() >= timeout_secs)
                .map(|(pid, (idx, _))| (*pid, *idx))
                .collect();
            for (pid, idx) in killed {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    let mut s2: libc::c_int = 0;
                    libc::waitpid(pid, &mut s2, 0);
                }
                inflight.remove(&pid);
                results[idx] = 3;
                timeout_n += 1;
            }
            std::thread::sleep(std::time::Duration::from_micros(500));
        }
    }

    let elapsed = start.elapsed();

    if let Some(jpath) = sub.get_one::<String>("json") {
        let mut out = String::new();
        for (i, p) in paths.iter().enumerate() {
            let v = match results[i] {
                1 => "PASS",
                2 => "FAIL",
                3 => "TIMEOUT",
                4 => "CRASH",
                _ => "UNKNOWN",
            };
            out.push_str(p);
            out.push('\t');
            out.push_str(v);
            out.push('\n');
        }
        std::fs::write(jpath, out).with_context(|| format!("writing {jpath}"))?;
    }

    println!(
        "test-batch: {total} fixtures in {:.2}s ({:.1}/s, jobs={jobs})\n  PASS={pass} FAIL={fail} TIMEOUT={timeout_n} CRASH={crash}",
        elapsed.as_secs_f64(),
        total as f64 / elapsed.as_secs_f64().max(1e-9),
    );
    Ok(())
}

pub(super) fn cmd_pytest(sub: &ArgMatches) -> Result<()> {
    use mamba::conformance::pytest_runner::{self, PytestOptions};

    let path = sub
        .get_one::<String>("path")
        .expect("clap enforces required");
    let timeout_secs: u64 = sub
        .get_one::<String>("timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let jobs: usize = sub
        .get_one::<String>("jobs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
        .max(1);
    let path_buf = std::path::PathBuf::from(path);
    if !path_buf.exists() {
        anyhow::bail!("path does not exist: {}", path_buf.display());
    }
    let mut opts = PytestOptions::new(path_buf);
    opts.timeout_secs = timeout_secs;
    opts.jobs = jobs;
    let summary = pytest_runner::run(&opts);
    if !summary.success() {
        std::process::exit(1);
    }
    Ok(())
}

pub(super) fn cmd_test(sub: &ArgMatches) -> Result<()> {
    let regen = sub.get_flag("regen-golden");
    let conformance = sub.get_flag("conformance");
    let path = sub.get_one::<String>("path");

    if let Some(p) = path {
        if conformance || regen {
            anyhow::bail!(
                "`mamba test <path>` is mutually exclusive with --conformance and --regen-golden"
            );
        }
        use mamba::conformance::pytest_runner::{self, PytestOptions};
        let path_buf = std::path::PathBuf::from(p);
        if !path_buf.exists() {
            anyhow::bail!("path does not exist: {}", path_buf.display());
        }
        let timeout_secs: u64 = sub
            .get_one::<String>("timeout")
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let jobs: usize = sub
            .get_one::<String>("jobs")
            .and_then(|s| s.parse().ok())
            .unwrap_or(4)
            .max(1);
        let mut opts = PytestOptions::new(path_buf);
        opts.timeout_secs = timeout_secs;
        opts.jobs = jobs;
        let summary = pytest_runner::run(&opts);
        if !summary.success() {
            std::process::exit(1);
        }
        return Ok(());
    }

    if regen {
        let dir = sub
            .get_one::<String>("dir")
            .map(|s| s.as_str())
            .unwrap_or(mamba::conformance::FIXTURES_ROOT);
        let status = std::process::Command::new("python3")
            .args(["tests/regen_golden.py", dir])
            .status()
            .map_err(|e| anyhow::anyhow!("failed to invoke python3: {e}"))?;
        if !status.success() {
            anyhow::bail!("regen_golden.py exited with {status}");
        }
        return Ok(());
    }

    if conformance {
        let dir = sub
            .get_one::<String>("dir")
            .map(|s| std::path::PathBuf::from(s))
            .unwrap_or_else(|| std::path::PathBuf::from(mamba::conformance::FIXTURES_ROOT));
        let category = sub.get_one::<String>("category").cloned();

        let opts = ConformanceOptions {
            conformance_dir: dir,
            category,
            ..Default::default()
        };
        println!("Running Mamba conformance suite...");
        if let Some(cat) = &opts.category {
            println!("  category filter: {cat}");
        }

        let (_, summary) = run_conformance_suite(&opts);
        println!();
        println!(
            "Results: {} total, {} passed, {} failed, {} xfailed, {} errors",
            summary.total, summary.passed, summary.failed, summary.xfailed, summary.errors
        );
        if summary.failed > 0 || summary.errors > 0 {
            std::process::exit(1);
        }
        return Ok(());
    }

    println!("Use 'mamba test --help' for usage information");
    Ok(())
}
