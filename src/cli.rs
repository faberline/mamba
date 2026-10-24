use anyhow::{bail, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use mamba::bench::{print_report, run_suite, BenchRunner, BenchSuite};
use mamba::conformance::{run_suite as run_conformance_suite, ConformanceOptions};
use mamba::driver::{Backend, CompilerConfig, CompilerSession, MambaConfig};
use mamba::pkgmanage::add as pkg_add;
use mamba::pkgmanage::audit as pkg_audit;
use mamba::pkgmanage::auth as pkg_auth;
use mamba::pkgmanage::builder as pkg_builder;
use mamba::pkgmanage::cache as pkg_cache;
use mamba::pkgmanage::export as pkg_export;
use mamba::pkgmanage::hash as pkg_hash;
use mamba::pkgmanage::index as pkg_index;
use mamba::pkgmanage::init as pkg_init;
use mamba::pkgmanage::install as pkg_install;
use mamba::pkgmanage::lock as pkg_lock;
use mamba::pkgmanage::migrate as pkg_migrate;
use mamba::pkgmanage::package as pkg_package;
use mamba::pkgmanage::pip as pkg_pip;
use mamba::pkgmanage::python as pkg_python;
use mamba::pkgmanage::remove as pkg_remove;
use mamba::pkgmanage::shell as pkg_shell;
use mamba::pkgmanage::sync as pkg_sync;
use mamba::pkgmanage::tool as pkg_tool;
use mamba::pkgmanage::tree as pkg_tree;
use mamba::pkgmanage::validate as pkg_validate;
use mamba::pkgmanage::venv as pkg_venv;
use mamba::pkgmanage::version as pkg_version;
use mamba::pkgmanage::workspace as pkg_workspace;

use crate::{quality_command, standard_cli};

mod bench;
mod misc;
mod run;
mod test;

fn cli() -> Command {
    Command::new("mamba")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Mamba - Force-typed Python compiler with native code generation")
        .subcommand(standard_cli::llm_command())
        .subcommand(standard_cli::upgrade_command())
        .subcommand(standard_cli::report_issue_command())
        .subcommand(quality_command::command_quality())
        .subcommand(
            Command::new("build")
                .about("Compile a Mamba source file or project")
                .arg(Arg::new("file").help("Source file (.py/.tp); omit to use entry_point from pyproject.toml [tool.mamba] (or a legacy mamba.toml)"))
                .arg(Arg::new("config").short('c').long("config").value_name("PATH").help("Path to a compiler config file (legacy mamba.toml layout)"))
                .arg(Arg::new("backend").short('b').long("backend").default_value("cranelift").help("Codegen backend: cranelift, llvm, wasm"))
                .arg(Arg::new("emit").long("emit").help("Dump intermediate: ast, hir, mir"))
                .arg(Arg::new("output").short('o').long("output").help("Output file path"))
                .arg(
                    Arg::new("aot")
                        .long("aot")
                        .action(ArgAction::SetTrue)
                        .help("Request AOT executable packaging (requires --output; backend pending)"),
                ),
        )
        .subcommand(
            Command::new("check")
                .about("Type-check a Mamba source file (no codegen)")
                .arg(Arg::new("file").required(true).help("Source file (.tp)")),
        )
        .subcommand(
            Command::new("run")
                .about("Run a Mamba source file/project, or run a command inside the project env after --")
                .arg(Arg::new("file").help("Source file (.py/.tp); omit to use entry_point from pyproject.toml [tool.mamba] (or a legacy mamba.toml)"))
                .arg(Arg::new("config").short('c').long("config").value_name("PATH").help("Path to a compiler config file (legacy mamba.toml layout)"))
                .arg(
                    Arg::new("compile")
                        .long("compile")
                        .action(ArgAction::SetTrue)
                        .help("Hand the file to the Mamba compiler instead of the resolved interpreter"),
                )
                .arg(
                    Arg::new("python")
                        .long("python")
                        .short('p')
                        .value_name("PYTHON")
                        .help("Interpreter to run the file with: a path, or a name looked up on PATH (overrides the project's .venv interpreter)"),
                )
                .arg(
                    Arg::new("no-project")
                        .long("no-project")
                        .action(ArgAction::SetTrue)
                        .help("Ignore the surrounding project: no sync preflight, no .venv on the child's PATH"),
                )
                .arg(
                    Arg::new("script-args")
                        .value_name("ARGS")
                        .help("Arguments passed through to the script")
                        .num_args(0..)
                        .allow_hyphen_values(true),
                )
                .arg(
                    Arg::new("command")
                        .value_name("COMMAND")
                        .help("Command and arguments to run inside the synced project environment; use after --")
                        .num_args(1..)
                        .last(true)
                        .allow_hyphen_values(true),
                ),
        )
        .subcommand(
            Command::new("bench")
                .about("Run the Mamba benchmark suite")
                .arg(Arg::new("compare").long("compare").value_name("ENGINE").help("Compare against: cpython"))
                .arg(Arg::new("filter").long("filter").value_name("KIND").help("Filter: numeric, recursion, workload"))
                .arg(Arg::new("file").value_name("FILE").help("Benchmark a single file"))
                .arg(Arg::new("fixtures").long("fixtures").value_name("DIR").default_missing_value(format!("{}/core/bench", mamba::conformance::FIXTURES_ROOT)).num_args(0..=1).help("Run fixture-based benchmarks"))
                .arg(Arg::new("json").long("json").value_name("PATH").help("Write timings + ratios to a JSON file (baseline format)"))
                .arg(Arg::new("check").long("check").value_name("PATH").help("Compare current run against the baseline JSON; fail on regression beyond --threshold")),
        )
        .subcommand(
            Command::new("test")
                .about("Run Mamba tests — Python-perspective, CPython-style runner")
                .arg(Arg::new("path").help("Test file or directory containing test_*.py; if omitted, requires --conformance or --regen-golden"))
                .arg(Arg::new("conformance").long("conformance").action(ArgAction::SetTrue).help("Run CPython 3.12 conformance suite"))
                .arg(Arg::new("category").long("category").value_name("NAME").help("Filter conformance category"))
                .arg(Arg::new("dir").long("dir").value_name("PATH").help("Conformance fixtures directory"))
                .arg(Arg::new("regen-golden").long("regen-golden").action(ArgAction::SetTrue).help("Regenerate .expected golden files"))
                .arg(Arg::new("timeout").long("timeout").value_name("SECS").default_value("60").help("Per-test timeout in seconds (path mode)"))
                .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").default_value("4").help("Max parallel subprocesses (path mode)")),
        )
        .subcommand(
            Command::new("test-batch")
                .about("Zygote-fork batch conformance runner: init the runtime ONCE, fork a worker per fixture (isolated, init amortized via COW). Reports per-path PASS/FAIL/TIMEOUT/CRASH.")
                .arg(Arg::new("paths").long("paths").value_name("FILE").help("File with one fixture path per line; omit to read paths from stdin"))
                .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").default_value("8").help("Max concurrent worker forks"))
                .arg(Arg::new("timeout").long("timeout").value_name("SECS").default_value("10").help("Per-fixture timeout in seconds"))
                .arg(Arg::new("json").long("json").value_name("PATH").help("Write per-path results as JSON lines (path\\tverdict)")),
        )
        .subcommand(
            Command::new("surface-report")
                .about("Compare mamba's registered Python-level surface for a stdlib/3p package against the typeshed stub")
                .arg(Arg::new("package").long("package").short('p').required(true).value_name("NAME").help("Package name (e.g. os, ssl, idna)"))
                .arg(Arg::new("typeshed").long("typeshed").value_name("PATH").help("Path to typeshed checkout (default: vendor/typeshed)"))
                .arg(Arg::new("mamba-src").long("mamba-src").value_name("PATH").help("Path to src (default: src)")),
        )
        .subcommand(
            Command::new("pytest")
                .about("Run pytest-under-mamba on a CPython Lib/test-style file or directory")
                .arg(Arg::new("path").required(true).help("Test file or directory containing test_*.py"))
                .arg(Arg::new("timeout").long("timeout").value_name("SECS").default_value("60").help("Per-test timeout in seconds"))
                .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").default_value("4").help("Max parallel subprocesses")),
        )
        .subcommand(
            Command::new("init")
                .about("Scaffold a new mamba project (pyproject.toml, .python-version, .gitignore, README.md, src/__init__.py)")
                .arg(Arg::new("path").help("Project directory; defaults to current working directory")),
        )
        .subcommand(
            Command::new("migrate")
                .about("Convert a retired mamba.toml into PEP 621 pyproject.toml (one-shot; removes mamba.toml)")
                .arg(Arg::new("path").help("Project directory; defaults to current working directory")),
        )
        .subcommand(
            Command::new("auth")
                .about("Manage package index credentials")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("login")
                        .about("Store plaintext credentials for a service")
                        .arg(Arg::new("service").required(true).value_name("SERVICE").help("Domain or URL of the service"))
                        .arg(Arg::new("username").long("username").short('u').value_name("USERNAME").help("Username to store; defaults to __token__"))
                        .arg(Arg::new("token").long("token").short('t').required(true).value_name("TOKEN").help("Token to store; use - to read from stdin")),
                )
                .subcommand(
                    Command::new("logout")
                        .about("Remove stored credentials for a service")
                        .arg(Arg::new("service").required(true).value_name("SERVICE").help("Domain or URL of the service"))
                        .arg(Arg::new("username").long("username").short('u').value_name("USERNAME").help("Username to remove; defaults to __token__")),
                )
                .subcommand(
                    Command::new("token")
                        .about("Print the stored token for a service")
                        .arg(Arg::new("service").required(true).value_name("SERVICE").help("Domain or URL of the service"))
                        .arg(Arg::new("username").long("username").short('u').value_name("USERNAME").help("Username to look up; defaults to __token__")),
                )
                .subcommand(
                    Command::new("dir")
                        .about("Print the credentials directory or one credential file path")
                        .arg(Arg::new("service").value_name("SERVICE").help("Optional service to resolve to a credential path"))
                        .arg(Arg::new("username").long("username").short('u').value_name("USERNAME").help("Username to resolve; defaults to __token__")),
                ),
        )
        .subcommand(
            Command::new("add")
                .about("Add a dependency to pyproject.toml and update mamba.lock")
                .arg(Arg::new("spec").num_args(0..).value_name("SPEC").help("Dependency specs or local wheel paths, e.g. foo==1.2.3 or ./wheels/foo-1.2.3-py3-none-any.whl (bare names require --index or explicit --index-url)"))
                .arg(Arg::new("requirements").short('r').long("requirements").value_name("FILE").action(ArgAction::Append).help("Add every requirement listed in FILE (pip requirements format; nested -r honoured)"))
                .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).conflicts_with_all(["group", "optional"]).help("Add to the `dev` dependency group instead of [project] dependencies"))
                .arg(Arg::new("group").long("group").value_name("NAME").conflicts_with("optional").help("Add to the named [dependency-groups] group"))
                .arg(Arg::new("optional").long("optional").value_name("EXTRA").help("Add to the named [project.optional-dependencies] extra"))
                .arg(Arg::new("provider").long("provider").value_name("PROVIDER").help("Explicit first-party provider for mamba-owned replacement packages, e.g. --provider mamba mamba-httpx-compat"))
                .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory (overrides $MAMBA_FROZEN_INDEX)"))
                .arg(Arg::new("index-url").long("index-url").value_name("URL").help("Explicit PyPI-compatible registry base URL or its /simple URL (overrides $MAMBA_INDEX_URL)"))
                .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Disallow network; require pinned version or local index")),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a dependency from pyproject.toml and update mamba.lock")
                .arg(Arg::new("name").required(true).num_args(1..).value_name("NAME").help("Dependency names (no version pin)"))
                .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).conflicts_with_all(["group", "optional"]).help("Remove from the `dev` dependency group only"))
                .arg(Arg::new("group").long("group").value_name("NAME").conflicts_with("optional").help("Remove from the named [dependency-groups] group only"))
                .arg(Arg::new("optional").long("optional").value_name("EXTRA").help("Remove from the named [project.optional-dependencies] extra only")),
        )
        .subcommand(
            Command::new("lock")
                .about("Regenerate mamba.lock from pyproject.toml; resolves transitive deps via a frozen index or explicit registry URL")
                .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory (overrides $MAMBA_FROZEN_INDEX)"))
                .arg(Arg::new("index-url").long("index-url").value_name("URL").help("Explicit PyPI-compatible registry base URL or its /simple URL (overrides $MAMBA_INDEX_URL)"))
                .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Disallow network; require frozen index"))
                .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("Fail if mamba.lock is missing or out of date without writing it"))
                .arg(Arg::new("upgrade").long("upgrade").short('U').action(ArgAction::SetTrue).help("Let every package move to its newest admissible version instead of keeping the pins mamba.lock already records"))
                .arg(Arg::new("upgrade-package").long("upgrade-package").short('P').value_name("NAME").action(ArgAction::Append).help("Let only NAME move; every other recorded pin stays (repeatable)")),
        )
        .subcommand(
            Command::new("audit")
                .about("Audit mamba.lock against a local advisory database")
                .arg(Arg::new("advisory-db").long("advisory-db").value_name("PATH").help("Local JSON advisory database (overrides $MAMBA_ADVISORY_DB)"))
                .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit JSON findings")),
        )
        .subcommand(
            Command::new("export")
                .about("Export mamba.lock to requirements.txt or pylock.toml")
                .arg(Arg::new("format").long("format").value_name("FORMAT").default_value("requirements-txt").help("requirements-txt | pylock.toml"))
                .arg(Arg::new("output-file").long("output-file").short('o').value_name("PATH").help("Write output to PATH; omit or use - for stdout"))
                .arg(Arg::new("no-hashes").long("no-hashes").action(ArgAction::SetTrue).help("Do not emit --hash continuations in requirements-txt output"))
                .arg(Arg::new("no-header").long("no-header").action(ArgAction::SetTrue).help("Do not emit the generated header in requirements-txt output"))
                .arg(Arg::new("no-emit-package").long("no-emit-package").value_name("NAME").action(ArgAction::Append).help("Exclude a package from requirements-txt output"))
                .arg(Arg::new("marker").long("marker").value_name("PEP508").help("Append a global PEP 508 environment marker to requirements-txt pins"))
                .arg(Arg::new("annotate").long("annotate").action(ArgAction::SetTrue).help("Annotate requirements pins with reverse dependency comments"))
                .arg(Arg::new("requires-python").long("requires-python").value_name("SPEC").help("Set requires-python in pylock.toml output"))
                .arg(Arg::new("environment").long("environment").value_name("MARKER").action(ArgAction::Append).help("Add an environment marker to pylock.toml output")),
        )
        .subcommand(
            Command::new("tree")
                .about("Display the dependency tree from mamba.lock")
                .arg(Arg::new("depth").long("depth").value_name("N").help("Maximum tree depth"))
                .arg(Arg::new("package").long("package").short('p').value_name("NAME").help("Render only the subtree rooted at NAME"))
                .arg(Arg::new("invert").long("invert").action(ArgAction::SetTrue).help("Show reverse dependency tree"))
                .arg(Arg::new("prune").long("prune").value_name("NAME").action(ArgAction::Append).help("Skip a dependency subtree"))
                .arg(Arg::new("no-dedupe").long("no-dedupe").action(ArgAction::SetTrue).help("Render repeated subtrees instead of marking duplicates")),
        )
        .subcommand(
            Command::new("version")
                .about("Read, set, or bump the PEP 621 [project].version in pyproject.toml")
                .arg(Arg::new("version").value_name("VERSION").help("Explicit PEP 440 version to set"))
                .arg(Arg::new("bump").long("bump").value_name("KIND").help("major | minor | patch | alpha | beta | rc | post | dev | release"))
                .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Print the next version without writing pyproject.toml")),
        )
        .subcommand(
            Command::new("package")
                .about("Build and validate Python package artifacts")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("build")
                        .about("Build deterministic pure-Python wheel and sdist artifacts from pyproject.toml")
                        .arg(Arg::new("project").long("project").value_name("DIR").help("Project directory; defaults to current directory"))
                        .arg(Arg::new("out-dir").long("out-dir").short('o').value_name("DIR").help("Artifact output directory; defaults to dist/ under the project"))
                        .arg(Arg::new("wheel").long("wheel").action(ArgAction::SetTrue).help("Build only a wheel unless --sdist is also passed"))
                        .arg(Arg::new("sdist").long("sdist").action(ArgAction::SetTrue).help("Build only an sdist unless --wheel is also passed"))
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit built artifact paths as JSON")),
                )
                .subcommand(
                    Command::new("publish")
                        .about("Publish package artifacts or validate upload payloads with --dry-run")
                        .arg(Arg::new("artifact").value_name("ARTIFACT").num_args(0..).action(ArgAction::Append).help("Wheel or sdist artifact; defaults to dist/*.whl and dist/*.tar.gz"))
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Validate payloads without uploading"))
                        .arg(Arg::new("repository").long("repository").value_name("NAME").help("Repository name in .pypirc, e.g. pypi or testpypi"))
                        .arg(Arg::new("publish-url").long("publish-url").value_name("URL").help("Upload endpoint URL; overrides repository lookup"))
                        .arg(Arg::new("username").long("username").short('u').value_name("USER").help("Upload username"))
                        .arg(Arg::new("password").long("password").short('p').value_name("PASSWORD").help("Upload password or token"))
                        .arg(Arg::new("pypirc").long("pypirc").value_name("FILE").help("Path to a .pypirc file; defaults to ~/.pypirc"))
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit publish summary as JSON")),
                ),
        )
        .subcommand(
            Command::new("publish")
                .about("Publish package artifacts or validate upload payloads with --dry-run")
                .arg(Arg::new("artifact").value_name("ARTIFACT").num_args(0..).action(ArgAction::Append).help("Wheel or sdist artifact; defaults to dist/*.whl and dist/*.tar.gz"))
                .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Validate payloads without uploading"))
                .arg(Arg::new("repository").long("repository").value_name("NAME").help("Repository name in .pypirc, e.g. pypi or testpypi"))
                .arg(Arg::new("publish-url").long("publish-url").value_name("URL").help("Upload endpoint URL; overrides repository lookup"))
                .arg(Arg::new("username").long("username").short('u').value_name("USER").help("Upload username"))
                .arg(Arg::new("password").long("password").short('p').value_name("PASSWORD").help("Upload password or token"))
                .arg(Arg::new("pypirc").long("pypirc").value_name("FILE").help("Path to a .pypirc file; defaults to ~/.pypirc"))
                .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit publish summary as JSON")),
        )
        .subcommand(
            Command::new("pip")
                .about("pip-compatible installed-environment inspection")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("compile")
                        .about("Compile requirements inputs into pinned requirements.txt or pylock.toml")
                        .arg(Arg::new("src").value_name("SRC_FILE").required(true).action(ArgAction::Append).num_args(1..).help("requirements input file, or - for stdin"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory for package requirements"))
                        .arg(Arg::new("index-url").long("index-url").value_name("URL").help("Explicit PyPI-compatible registry base URL (overrides $MAMBA_INDEX_URL)"))
                        .arg(Arg::new("output-file").long("output-file").short('o').value_name("FILE").help("Write compiled output to FILE; stdout when omitted"))
                        .arg(Arg::new("format").long("format").value_name("FORMAT").help("requirements.txt | pylock.toml"))
                        .arg(Arg::new("generate-hashes").long("generate-hashes").action(ArgAction::SetTrue).help("Include sha256 hashes in requirements.txt output"))
                        .arg(Arg::new("no-header").long("no-header").action(ArgAction::SetTrue).help("Omit the generated requirements.txt header"))
                        .arg(Arg::new("no-annotate").long("no-annotate").action(ArgAction::SetTrue).help("Omit # via dependency annotations"))
                        .arg(Arg::new("no-deps").long("no-deps").action(ArgAction::SetTrue).help("Only emit root requirements"))
                        .arg(Arg::new("no-emit-package").long("no-emit-package").value_name("NAME").action(ArgAction::Append).help("Omit a package from the compiled output")),
                )
                .subcommand(
                    Command::new("install")
                        .about("Install wheels or frozen-index package pins into an environment")
                        .arg(Arg::new("spec").value_name("REQ_OR_WHEEL").action(ArgAction::Append).num_args(0..).help("Package requirement or wheel path"))
                        .arg(Arg::new("requirement").long("requirement").short('r').value_name("FILE").action(ArgAction::Append).help("Install requirements from a requirements.txt file"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory for package requirements"))
                        .arg(Arg::new("index-url").long("index-url").value_name("URL").help("Explicit PyPI-compatible registry base URL (overrides $MAMBA_INDEX_URL)"))
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages"))
                        .arg(Arg::new("python").long("python").short('p').value_name("PYTHON").help("Python executable for console-script wrappers; defaults to the project's .venv interpreter, else python3")),
                )
                .subcommand(
                    Command::new("sync")
                        .about("Sync an environment to requirements from frozen-index package pins")
                        .arg(Arg::new("src").value_name("SRC_FILE").required(true).action(ArgAction::Append).num_args(1..).help("requirements.txt file to sync"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory for package requirements"))
                        .arg(Arg::new("index-url").long("index-url").value_name("URL").help("Explicit PyPI-compatible registry base URL (overrides $MAMBA_INDEX_URL)"))
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages"))
                        .arg(Arg::new("python").long("python").short('p').value_name("PYTHON").help("Python executable for console-script wrappers; defaults to the project's .venv interpreter, else python3")),
                )
                .subcommand(
                    Command::new("uninstall")
                        .about("Uninstall packages from an environment using installed RECORD files")
                        .arg(Arg::new("package").value_name("PACKAGE").required(true).action(ArgAction::Append).num_args(1..).help("Package name to uninstall"))
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages")),
                )
                .subcommand(
                    Command::new("list")
                        .about("List installed distributions from site-packages")
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages"))
                        .arg(Arg::new("format").long("format").value_name("FORMAT").default_value("columns").help("columns | freeze"))
                        .arg(Arg::new("no-header").long("no-header").action(ArgAction::SetTrue).help("Omit column headers"))
                        .arg(Arg::new("sort-by-version").long("sort-by-version").action(ArgAction::SetTrue).help("Sort rows by version instead of package name")),
                )
                .subcommand(
                    Command::new("freeze")
                        .about("Emit installed distributions as requirements pins")
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages")),
                )
                .subcommand(
                    Command::new("show")
                        .about("Show metadata for one installed distribution")
                        .arg(Arg::new("name").required(true).help("Package name"))
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages")),
                )
                .subcommand(
                    Command::new("tree")
                        .about("Display the dependency tree for an installed environment")
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages"))
                        .arg(Arg::new("depth").long("depth").short('d').value_name("N").help("Maximum display depth"))
                        .arg(Arg::new("package").long("package").value_name("NAME").help("Display only the specified package"))
                        .arg(Arg::new("invert").long("invert").alias("reverse").action(ArgAction::SetTrue).help("Show reverse dependencies"))
                        .arg(Arg::new("prune").long("prune").value_name("NAME").action(ArgAction::Append).help("Prune the given package from the display"))
                        .arg(Arg::new("no-dedupe").long("no-dedupe").action(ArgAction::SetTrue).help("Repeat already-rendered dependency subtrees")),
                )
                .subcommand(
                    Command::new("check")
                        .about("Check installed distribution requirements")
                        .arg(Arg::new("site-packages").long("site-packages").value_name("DIR").help("site-packages directory; defaults to .venv/site-packages")),
                ),
        )
        .subcommand(
            Command::new("venv")
                .about("Create or remove a PEP 405 virtual environment")
                .arg(Arg::new("path").value_name("PATH").help("Environment path for bare `mamba venv`; defaults to .venv"))
                .arg(Arg::new("python").long("python").value_name("PYTHON").help("Python interpreter to seed the venv; defaults to first python on PATH"))
                .arg(Arg::new("system-site-packages").long("system-site-packages").action(ArgAction::SetTrue).help("Give the venv access to system site-packages"))
                .arg(Arg::new("copies").long("copies").action(ArgAction::SetTrue).help("Copy the interpreter instead of symlinking when supported"))
                .arg(Arg::new("seed").long("seed").action(ArgAction::SetTrue).help("Let Python seed pip into the venv"))
                .arg(Arg::new("prompt").long("prompt").value_name("PROMPT").help("Prompt name passed to python -m venv"))
                .arg(Arg::new("clear").long("clear").action(ArgAction::SetTrue).help("Replace an existing venv at the target path"))
                .subcommand(
                    Command::new("create")
                        .about("Create a virtual environment")
                        .arg(Arg::new("path").value_name("PATH").help("Environment path; defaults to .venv"))
                        .arg(Arg::new("python").long("python").value_name("PYTHON").help("Python interpreter to seed the venv; defaults to first python on PATH"))
                        .arg(Arg::new("system-site-packages").long("system-site-packages").action(ArgAction::SetTrue).help("Give the venv access to system site-packages"))
                        .arg(Arg::new("copies").long("copies").action(ArgAction::SetTrue).help("Copy the interpreter instead of symlinking when supported"))
                        .arg(Arg::new("seed").long("seed").action(ArgAction::SetTrue).help("Let Python seed pip into the venv"))
                        .arg(Arg::new("prompt").long("prompt").value_name("PROMPT").help("Prompt name passed to python -m venv"))
                        .arg(Arg::new("clear").long("clear").action(ArgAction::SetTrue).help("Replace an existing venv at the target path")),
                )
                .subcommand(
                    Command::new("remove")
                        .about("Remove a virtual environment only when pyvenv.cfg is present")
                        .arg(Arg::new("path").value_name("PATH").help("Environment path; defaults to .venv")),
                ),
        )
        .subcommand(
            Command::new("python")
                .about("Discover local Python interpreters and manage .python-version pins")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("install")
                        .about("Install a managed Python from a local source interpreter or standalone archive")
                        .arg(Arg::new("request").value_name("REQUEST").help("Version request such as 3, 3.12, or 3.12.7; defaults to .python-version or any"))
                        .arg(Arg::new("source").long("source").value_name("PYTHON").help("Local Python executable to register; defaults to a matching PATH interpreter"))
                        .arg(Arg::new("url").long("url").value_name("URL").help("Standalone Python archive URL; overrides python-build-standalone URL composition"))
                        .arg(Arg::new("release-tag").long("release-tag").value_name("TAG").help("python-build-standalone release tag used to compose a GitHub archive URL"))
                        .arg(Arg::new("sha256").long("sha256").value_name("HEX").help("Expected sha256 for the downloaded archive")),
                )
                .subcommand(
                    Command::new("download")
                        .about("Download/register a managed Python from a standalone archive")
                        .arg(Arg::new("request").value_name("REQUEST").help("Version request such as 3, 3.12, or 3.12.7; defaults to .python-version or any"))
                        .arg(Arg::new("source").long("source").value_name("PYTHON").help("Local Python executable to register instead of downloading"))
                        .arg(Arg::new("url").long("url").value_name("URL").help("Standalone Python archive URL; overrides python-build-standalone URL composition"))
                        .arg(Arg::new("release-tag").long("release-tag").value_name("TAG").help("python-build-standalone release tag used to compose a GitHub archive URL"))
                        .arg(Arg::new("sha256").long("sha256").value_name("HEX").help("Expected sha256 for the downloaded archive")),
                )
                .subcommand(
                    Command::new("uninstall")
                        .about("Remove managed Python installations matching a request")
                        .arg(Arg::new("request").value_name("REQUEST").help("Version request such as 3, 3.12, or 3.12.7; defaults to .python-version or any")),
                )
                .subcommand(
                    Command::new("list")
                        .about("List Python interpreters discovered on PATH")
                        .arg(
                            Arg::new("json")
                                .long("json")
                                .action(ArgAction::SetTrue)
                                .help("Emit JSON"),
                        ),
                )
                .subcommand(
                    Command::new("find")
                        .about("Print the best installed Python matching a request or .python-version")
                        .arg(
                            Arg::new("request")
                                .value_name("REQUEST")
                                .help("Version request such as 3, 3.12, or 3.12.7"),
                        ),
                )
                .subcommand(
                    Command::new("pin")
                        .about("Write a pyenv/uv-compatible .python-version file")
                        .arg(
                            Arg::new("request")
                                .required(true)
                                .value_name("REQUEST")
                                .help("Version request such as 3, 3.12, or 3.12.7"),
                        )
                        .arg(
                            Arg::new("project")
                                .long("project")
                                .value_name("DIR")
                                .help("Project directory; defaults to current directory"),
                        ),
                )
                .subcommand(
                    Command::new("dir")
                        .about("Print the managed Python install directory")
                        .arg(Arg::new("bin").long("bin").action(ArgAction::SetTrue).help("Print the managed Python executable directory")),
                )
                .subcommand(
                    Command::new("update-shell")
                        .about("Print a managed PATH init block for Python executables")
                        .arg(Arg::new("shell").long("shell").value_name("SHELL").help("bash | zsh | fish | powershell | cmd | nushell | elvish; defaults to bash"))
                        .arg(Arg::new("bin-dir").long("bin-dir").value_name("DIR").help("Launcher directory to prepend; defaults under the Python install root")),
                ),
        )
        .subcommand(
            Command::new("shell")
                .about("Print shell integration snippets for mamba-managed bin directories")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("path")
                        .about("Print a shell-specific PATH prepend snippet")
                        .arg(Arg::new("shell").long("shell").value_name("SHELL").help("bash | zsh | fish | powershell | cmd | nushell | elvish; defaults to $SHELL detection"))
                        .arg(Arg::new("bin-dir").long("bin-dir").value_name("DIR").help("Directory to prepend; defaults to the mamba tool bin directory")),
                )
                .subcommand(
                    Command::new("init")
                        .about("Print an idempotent managed shell init block")
                        .arg(Arg::new("shell").long("shell").value_name("SHELL").help("bash | zsh | fish | powershell | cmd | nushell | elvish; defaults to $SHELL detection"))
                        .arg(Arg::new("bin-dir").long("bin-dir").value_name("DIR").help("Directory to prepend; defaults to the mamba tool bin directory")),
                ),
        )
        .subcommand(
            Command::new("workspace")
                .about("Inspect uv-compatible [tool.uv.workspace] membership and metadata")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("metadata")
                        .about("View metadata about the current workspace")
                        .arg(Arg::new("root").long("root").value_name("DIR").help("Workspace root directory; defaults to current directory")),
                )
                .subcommand(
                    Command::new("dir")
                        .about("Display the path of the workspace root or one member")
                        .arg(Arg::new("root").long("root").value_name("DIR").help("Workspace root directory; defaults to current directory"))
                        .arg(Arg::new("package").long("package").value_name("NAME").help("Display the path to a specific workspace package")),
                )
                .subcommand(
                    Command::new("list")
                        .about("List workspace members from pyproject.toml")
                        .arg(Arg::new("root").long("root").value_name("DIR").help("Workspace root directory; defaults to current directory"))
                        .arg(Arg::new("paths").long("paths").action(ArgAction::SetTrue).help("Show member paths instead of package names"))
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit JSON")),
                ),
        )
        .subcommand(
            Command::new("index")
                .about("Build frozen local package indexes from wheel artifacts")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("build")
                        .about("Build a frozen local index from wheel files or directories")
                        .arg(Arg::new("out").long("out").short('o').required(true).value_name("DIR").help("Output frozen index directory"))
                        .arg(Arg::new("paths").required(true).num_args(1..).value_name("WHEEL_OR_DIR").help("Wheel file(s) or directories to scan recursively")),
                ),
        )
        .subcommand(
            Command::new("sync")
                .about("Converge `.venv`/site-packages to mamba.lock (idempotent; second run is a no-op)")
                .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").help("Max concurrent downloads (overrides $MAMBA_JOBS; default 8)"))
                .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("Fail if the environment is not already synchronized without mutating it"))
                .arg(Arg::new("no-dev").long("no-dev").action(ArgAction::SetTrue).conflicts_with("dev").help("Leave the `dev` dependency group out"))
                .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Include the `dev` dependency group (the default)"))
                .arg(Arg::new("group").long("group").value_name("NAME").action(ArgAction::Append).help("Also install the named [dependency-groups] group (repeatable)"))
                .arg(Arg::new("all-groups").long("all-groups").action(ArgAction::SetTrue).help("Install every dependency group"))
                .arg(Arg::new("extra").long("extra").value_name("EXTRA").action(ArgAction::Append).help("Also install the named [project.optional-dependencies] extra (repeatable)"))
                .arg(Arg::new("all-extras").long("all-extras").action(ArgAction::SetTrue).help("Install every extra"))
                .arg(Arg::new("locked").long("locked").action(ArgAction::SetTrue).conflicts_with("frozen").help("Refuse to sync when mamba.lock is out of date with pyproject.toml"))
                .arg(Arg::new("frozen").long("frozen").action(ArgAction::SetTrue).help("Sync from mamba.lock exactly as it is, without checking it against pyproject.toml (mamba's default; accepted for uv parity)")),
        )
        .subcommand(
            Command::new("pkgmgr-validate")
                .about("Drive the offline pkgmgr workflow families and emit a summary")
                .arg(Arg::new("include-live-network").long("include-live-network").action(ArgAction::SetTrue).help("Also walk opt-in live-network workflows (never blocks)"))
                .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON to stdout")),
        )
        .subcommand(
            Command::new("install")
                .about("Install a tool from the frozen local index into $MAMBA_TOOLS_DIR")
                .arg(Arg::new("name").help("Package name (omit when using --list)"))
                .arg(Arg::new("version").long("version").value_name("X.Y.Z").help("Pin a specific version (default: latest in index)"))
                .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory (overrides $MAMBA_FROZEN_INDEX)"))
                .arg(Arg::new("list").long("list").action(ArgAction::SetTrue).help("List installed tools"))
                .arg(Arg::new("uninstall").long("uninstall").value_name("NAME").help("Uninstall a tool")),
        )
        .subcommand(
            Command::new("tool")
                .about("Run and install commands provided by Python packages")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("run")
                        .about("Run an installed tool, installing from a frozen index when needed")
                        .arg(Arg::new("name").required(true).help("Tool/package name"))
                        .arg(Arg::new("version").long("version").value_name("X.Y.Z").help("Version to install when the tool is missing"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index for auto-install when missing"))
                        .arg(Arg::new("args").value_name("ARGS").num_args(0..).trailing_var_arg(true).allow_hyphen_values(true).help("Arguments passed to the tool")),
                )
                .subcommand(
                    Command::new("install")
                        .about("Install a tool from a frozen local index")
                        .arg(Arg::new("name").required(true).help("Package name"))
                        .arg(Arg::new("version").long("version").value_name("X.Y.Z").help("Pin a specific version; default is latest in index"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory (overrides $MAMBA_FROZEN_INDEX)")),
                )
                .subcommand(
                    Command::new("upgrade")
                        .about("Upgrade a tool to the latest version in a frozen local index")
                        .arg(Arg::new("name").required(true).help("Package name"))
                        .arg(Arg::new("index").long("index").value_name("DIR").help("Frozen local index directory (overrides $MAMBA_FROZEN_INDEX)")),
                )
                .subcommand(Command::new("list").about("List installed tools"))
                .subcommand(
                    Command::new("uninstall")
                        .about("Uninstall a tool")
                        .arg(Arg::new("name").required(true).help("Package name")),
                )
                .subcommand(Command::new("dir").about("Print the tool install directory"))
                .subcommand(
                    Command::new("update-shell")
                        .about("Print a managed PATH init block for tool launchers")
                        .arg(Arg::new("shell").long("shell").value_name("SHELL").help("bash | zsh | fish | powershell | cmd | nushell | elvish; defaults to bash"))
                        .arg(Arg::new("bin-dir").long("bin-dir").value_name("DIR").help("Launcher directory to prepend; defaults under the tool root")),
                ),
        )
        .subcommand(
            Command::new("hash")
                .about("Compute a content-addressed digest of one or more files")
                .arg(Arg::new("path").required(true).num_args(1..).help("File(s) to hash"))
                .arg(Arg::new("algorithm").long("algorithm").short('a').value_name("ALGO").help("sha256 (default) | sha384 | sha512")),
        )
        .subcommand(
            Command::new("cache")
                .about("Inspect and maintain the package cache root (respects $MAMBA_CACHE_DIR)")
                .subcommand(Command::new("dir").about("Print the resolved cache root"))
                .subcommand(
                    Command::new("size")
                        .about("Print the cache's total byte size")
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit JSON")),
                )
                .subcommand(
                    Command::new("info")
                        .about("Print cache entry counts and byte totals")
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit JSON")),
                )
                .subcommand(Command::new("clean").about("Remove every entry under the cache root"))
                .subcommand(
                    Command::new("prune")
                        .about("Remove cache entries by age, size, or package name")
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Report selected entries without deleting them"))
                        .arg(Arg::new("older-than-seconds").long("older-than-seconds").value_name("SECONDS").help("Remove entries older than this age"))
                        .arg(Arg::new("max-size").long("max-size").value_name("BYTES").help("Evict oldest entries until the eligible cache fits this size"))
                        .arg(Arg::new("package").long("package").value_name("NAME").action(ArgAction::Append).help("Only prune metadata/artifacts for this package"))
                        .arg(Arg::new("all-unknown").long("all-unknown").action(ArgAction::SetTrue).help("Allow pruning unknown cache files")),
                ),
        )
        .subcommand(
            Command::new("generate-shell-completion")
                .about("Generate a shell completion script")
                .arg(
                    Arg::new("shell")
                        .required(true)
                        .value_parser(["bash", "zsh", "fish", "powershell", "elvish"])
                        .help("Shell to generate completion for"),
                ),
        )
}

pub(crate) fn run() -> Result<()> {
    // Quality uses its own Rust service and does not need the compiler runtime.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == "quality")
    {
        let matches = cli().get_matches();
        return quality_command::cmd_quality(
            matches
                .subcommand_matches("quality")
                .expect("quality command"),
        );
    }

    // Install the ObjectOps callback table before any binding code can fire.
    // Bindings that depend only on mamba-registry call through
    // `registry::ops()` to allocate / inspect mamba objects and to raise
    // exceptions; the table must be populated before the first such call.
    mamba::runtime::registry_bridge::install();
    // Trip loudly if a force-link `use … as _;` was dropped — linkme misses
    // are otherwise silent and only surface as `ModuleNotFoundError` deep
    // inside user code.
    pkg_builder::assert_all_registered();

    // Python-style direct invocation, intercepted before clap parses
    // subcommands:
    //   mamba -c "<code>"   — run an inline program (CPython `python -c`)
    //   mamba <file>.py     — run a script (CPython `python file.py`)
    // `sys.executable` points at this binary, so CPython-conformance
    // fixtures spawn `[sys.executable, "-c", ...]` and expect both forms.
    {
        let argv: Vec<String> = std::env::args().collect();
        if argv.len() >= 2 {
            if argv[1] == "-c" {
                let Some(code) = argv.get(2) else {
                    eprintln!("Argument expected for the -c option");
                    std::process::exit(2);
                };
                return run::run_inline_source(code, "<string>");
            }
            if argv[1].ends_with(".py") && std::path::Path::new(&argv[1]).exists() {
                return run::run_script_path(&argv[1]);
            }
        }
    }

    let matches = cli().get_matches();

    match matches.subcommand() {
        Some(("quality", sub)) => quality_command::cmd_quality(sub),
        Some(("llm", sub)) => standard_cli::run_llm(sub),
        Some(("upgrade", sub)) => standard_cli::run_upgrade(sub),
        Some(("report-issue", sub)) => standard_cli::run_report_issue(sub),
        Some(("build", sub)) => pkg_builder::cmd_build(sub),
        Some(("check", sub)) => run::cmd_check(sub),
        Some(("run", sub)) => run::cmd_run(sub),
        Some(("bench", sub)) => bench::cmd_bench(sub),
        Some(("test", sub)) => test::cmd_test(sub),
        Some(("test-batch", sub)) => test::cmd_test_batch(sub),
        Some(("surface-report", sub)) => misc::cmd_surface_report(sub),
        Some(("pytest", sub)) => test::cmd_pytest(sub),
        Some(("init", sub)) => pkg_init::cmd_init(sub),
        Some(("migrate", sub)) => pkg_migrate::cmd_migrate(sub),
        Some(("auth", sub)) => pkg_auth::cmd_auth(sub),
        Some(("add", sub)) => pkg_add::cmd_add(sub),
        Some(("remove", sub)) => pkg_remove::cmd_remove(sub),
        Some(("lock", sub)) => pkg_lock::cmd_lock(sub),
        Some(("audit", sub)) => pkg_audit::cmd_audit(sub),
        Some(("export", sub)) => pkg_export::cmd_export(sub),
        Some(("tree", sub)) => pkg_tree::cmd_tree(sub),
        Some(("version", sub)) => pkg_version::cmd_version(sub),
        Some(("package", sub)) => pkg_package::cmd_package(sub),
        Some(("publish", sub)) => pkg_package::cmd_publish(sub),
        Some(("pip", sub)) => pkg_pip::cmd_pip(sub),
        Some(("venv", sub)) => pkg_venv::cmd_venv(sub),
        Some(("python", sub)) => pkg_python::cmd_python(sub),
        Some(("shell", sub)) => pkg_shell::cmd_shell(sub),
        Some(("workspace", sub)) => pkg_workspace::cmd_workspace(sub),
        Some(("index", sub)) => pkg_index::cmd_index(sub),
        Some(("sync", sub)) => pkg_sync::cmd_sync(sub),
        Some(("cache", sub)) => pkg_cache::cmd_cache(sub),
        Some(("hash", sub)) => pkg_hash::cmd_hash(sub),
        Some(("install", sub)) => pkg_install::cmd_install(sub),
        Some(("tool", sub)) => pkg_tool::cmd_tool(sub),
        Some(("pkgmgr-validate", sub)) => pkg_validate::cmd_validate(sub),
        Some(("generate-shell-completion", sub)) => misc::cmd_generate_shell_completion(sub),
        _ => {
            cli().print_help().ok();
            Ok(())
        }
    }
}
