use super::*;

pub(super) fn cmd_check(sub: &ArgMatches) -> Result<()> {
    let file = sub.get_one::<String>("file").unwrap();
    let config = CompilerConfig::default();
    let mut session = CompilerSession::new(config);
    match session.check(file) {
        Ok(()) => println!("Type check passed"),
        Err(e) => {
            eprintln!("{}", session.render_error(&e));
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Map a failed program run to a CPython-style process exit. `SystemExit`
/// carries the exit status (no argument → 0, int → that code, anything
/// else → message on stderr + 1); every other uncaught exception renders
/// and exits 1.
fn exit_like_cpython(session: &CompilerSession, err: &mamba::error::MambaError) -> ! {
    let rendered = session.render_error(err);
    let stripped = rendered.strip_prefix("error: ").unwrap_or(&rendered);
    // A program with no executable top-level statements (`pass`, comments
    // only) produces no JIT entry point; CPython exits 0 on such input.
    if stripped.contains("no entry point found") {
        std::process::exit(0);
    }
    if let Some(rest) = stripped.strip_prefix("SystemExit") {
        let payload = rest.trim_start_matches(':').trim();
        if payload.is_empty() || payload == "None" {
            std::process::exit(0);
        }
        if let Ok(code) = payload.parse::<i32>() {
            std::process::exit(code);
        }
        eprintln!("{payload}");
        std::process::exit(1);
    }
    eprintln!("{rendered}");
    std::process::exit(1);
}

/// `mamba -c "<code>"` — compile and run an inline program, mirroring
/// CPython's `python -c`. Exit code follows the program (SystemExit /
/// uncaught exception → non-zero), no package-manager preflight.
pub(super) fn run_inline_source(code: &str, name: &str) -> Result<()> {
    let config = CompilerConfig {
        backend: Backend::CraneliftJit,
        ..Default::default()
    };
    let mut session = CompilerSession::new(config);
    match session.run_source(code, name) {
        Ok(()) => Ok(()),
        Err(e) => exit_like_cpython(&session, &e),
    }
}

/// `mamba <file>.py` — run a script directly, mirroring `python file.py`.
pub(super) fn run_script_path(path: &str) -> Result<()> {
    let config = CompilerConfig {
        backend: Backend::CraneliftJit,
        ..Default::default()
    };
    let mut session = CompilerSession::new(config);
    match session.run(path) {
        Ok(()) => Ok(()),
        Err(e) => exit_like_cpython(&session, &e),
    }
}

/// Directory to remove on process exit when `MAMBA_SCRATCH_CWD=1` isolation
/// was entered. Cleanup runs via a `libc::atexit` hook rather than `Drop`
/// because most `cmd_run` error paths call `std::process::exit()` directly,
/// which skips destructors but still invokes atexit handlers.
static SCRATCH_CWD_TO_CLEAN: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

extern "C" fn cleanup_scratch_cwd() {
    if let Some(dir) = SCRATCH_CWD_TO_CLEAN.get() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Issue #1558: opt-in scratch-CWD isolation for the agent/harness direct-run
/// path (`MAMBA_SCRATCH_CWD=1 mamba run <fixture>`). CPython-conformance
/// fixtures use `test.support.os_helper.TESTFN`, a CWD-relative scratch
/// filename by design (relative-path I/O assertions are part of the fixture
/// semantics) — so the fix relocates the CWD, not the filename, mirroring
/// `conformance::ScratchCwd`'s regrtest-style throwaway CWD. No-op unless the
/// env var is set, so default `mamba run` behavior (real programs run in the
/// user's CWD) is unchanged.
fn maybe_enter_scratch_cwd() {
    if std::env::var("MAMBA_SCRATCH_CWD").ok().as_deref() != Some("1") {
        return;
    }
    let scratch = std::env::temp_dir().join(format!(
        "mamba_run_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if std::fs::create_dir_all(&scratch).is_ok() && std::env::set_current_dir(&scratch).is_ok() {
        if SCRATCH_CWD_TO_CLEAN.set(scratch).is_ok() {
            unsafe {
                libc::atexit(cleanup_scratch_cwd);
            }
        }
    }
}

pub(super) fn cmd_run(sub: &ArgMatches) -> Result<()> {
    use std::io::Read as _;

    // Package-manager preflight: when invoked inside a mamba project
    // with a populated lockfile, require `.venv` to be in sync before
    // running anything. See src/pkgmanage/run.rs.
    let cwd_for_preflight = std::env::current_dir().context("getcwd")?;
    // `--no-project` is uv's: the file runs as if no project were here, so
    // the sync preflight is skipped and the child's PATH is left alone.
    let no_project = sub.get_flag("no-project");
    let preflight_mode = if no_project {
        mamba::pkgmanage::run::Mode::Legacy
    } else {
        match mamba::pkgmanage::run::preflight(&cwd_for_preflight) {
            Ok(mode) => mode,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    };
    let python_override = sub.get_one::<String>("python").cloned();

    if let Some(command) = sub.get_many::<String>("command") {
        let args: Vec<String> = command.cloned().collect();
        return cmd_run_command_mode(args, &cwd_for_preflight, &preflight_mode, no_project);
    }

    let project_config: Option<MambaConfig> =
        if let Some(cfg_path) = sub.get_one::<String>("config") {
            Some(MambaConfig::from_file(std::path::Path::new(cfg_path))?)
        } else {
            let cwd = std::env::current_dir().context("getcwd")?;
            MambaConfig::discover(&cwd).map(|(cfg, _)| cfg)
        };

    let file_arg = sub.get_one::<String>("file");
    let entry_from_config = project_config
        .as_ref()
        .and_then(|c| c.entry_point().map(|s| s.to_string()));
    let file: String = match (file_arg, entry_from_config) {
        (Some(f), _) => f.clone(),
        (None, Some(ep)) => ep,
        (None, None) => anyhow::bail!(
            "no source file specified and no pyproject.toml [tool.mamba] or mamba.toml found; pass a file or cd into a project directory"
        ),
    };

    let script_args: Vec<String> = sub
        .get_many::<String>("script-args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let compile_flag = sub.get_flag("compile");

    // Resolve a relative `file` (and the `-` stdin sentinel is left as-is)
    // against the CURRENT cwd before any scratch-CWD chdir below — config
    // discovery above already needed the original cwd, so scratch entry is
    // the last thing before we start running the program.
    let file = std::fs::canonicalize(&file)
        .map(|p| p.display().to_string())
        .unwrap_or(file);

    // Default route: the project's own interpreter (or the PATH interpreter
    // outside a project) runs the file directly. `--compile` and the `-`
    // stdin sentinel are the only ways to reach the Mamba compiler.
    if !mamba::pkgmanage::run::routes_to_compiler(compile_flag, &file) {
        return cmd_run_interpreter_mode(
            &file,
            &script_args,
            &cwd_for_preflight,
            &preflight_mode,
            python_override.as_deref(),
            no_project,
        );
    }

    maybe_enter_scratch_cwd();

    let config = CompilerConfig {
        backend: Backend::CraneliftJit,
        project_config,
        ..Default::default()
    };
    let mut session = CompilerSession::new(config);

    // REQ: R1 — when the argument is the stdin sentinel "-", read from stdin.
    if file == "-" {
        // REQ: R3 — reject when stdin is a TTY (no piped input provided).
        use std::io::IsTerminal as _;
        if std::io::stdin().is_terminal() {
            eprintln!("error: stdin is a tty, no script provided");
            std::process::exit(1);
        }

        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .context("reading stdin")?;

        // REQ: R2 — exit behaviour mirrors `mamba run <file.py>`.
        match session.run_source(&source, "<stdin>") {
            Ok(()) => {}
            Err(e) => {
                eprintln!("{}", session.render_error(&e));
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // REQ: R4 — existing file-path behaviour is preserved.
    match session.run(&file) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", session.render_error(&e));
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Default route for `mamba run <file>`: spawn the resolved interpreter
/// (the project's own `.venv` interpreter, or the `PATH` interpreter
/// outside a project) with the file and its trailing arguments, stdio
/// inherited, and propagate the child's exit code unchanged.
fn cmd_run_interpreter_mode(
    file: &str,
    script_args: &[String],
    project_dir: &std::path::Path,
    preflight_mode: &mamba::pkgmanage::run::Mode,
    python_override: Option<&str>,
    no_project: bool,
) -> Result<()> {
    let path_value = std::env::var_os("PATH");
    let interpreter = match python_override {
        Some(request) => mamba::pkgmanage::run::resolve_python_override(
            request,
            project_dir,
            path_value.as_deref(),
        ),
        None => mamba::pkgmanage::run::resolve_run_interpreter(
            project_dir,
            preflight_mode,
            path_value.as_deref(),
        ),
    };
    let interpreter = match interpreter {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut command = std::process::Command::new(&interpreter);
    command.arg(file).args(script_args).current_dir(project_dir);
    if !no_project {
        mamba::pkgmanage::run::configure_command_environment(
            &mut command,
            project_dir,
            preflight_mode,
        );
    }

    let status = command
        .status()
        .with_context(|| format!("run `{file}` with {}", interpreter.display()))?;
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    std::process::exit(1);
}

fn cmd_run_command_mode(
    args: Vec<String>,
    project_dir: &std::path::Path,
    preflight_mode: &mamba::pkgmanage::run::Mode,
    no_project: bool,
) -> Result<()> {
    if args.is_empty() {
        bail!("no command specified after --");
    }

    let mut command = std::process::Command::new(&args[0]);
    command.args(&args[1..]).current_dir(project_dir);
    if !no_project {
        mamba::pkgmanage::run::configure_command_environment(
            &mut command,
            project_dir,
            preflight_mode,
        );
    }

    let status = command
        .status()
        .with_context(|| format!("run command `{}`", args[0]))?;
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    std::process::exit(1);
}
