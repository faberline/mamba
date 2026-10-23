//! Concurrent cache admission and dependency changes retain exact reports.
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};

struct ProbeEngine {
    calls: Arc<AtomicUsize>,
    gate: Option<Arc<Barrier>>,
    mutate: Option<PathBuf>,
    completion: CompletionStatus,
}

impl QualityEngine for ProbeEngine {
    fn check(
        &self,
        workspace: &QualityWorkspace,
        _: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        if let Some(path) = &self.mutate {
            fs::write(path, "VALUE: str\n").unwrap();
        }
        ToolReport {
            tool,
            reference_version: tool.reference_version().into(),
            completion: self.completion,
            reference_exit_code: Some(0),
            checked_files: workspace.sources.len(),
            diagnostics: Vec::new(),
            remaining: Vec::new(),
        }
    }

    fn transform(&self, _: &QualityWorkspace, _: &FixedProfile, _: QualityTool) -> TransformResult {
        unreachable!("Cache admission does not transform source")
    }

    fn validate(
        &self,
        _: &SourceSnapshot,
        _: &str,
        _: &QualityWorkspace,
        _: bool,
    ) -> Result<(), QualityIssue> {
        unreachable!("Cache admission does not validate candidates")
    }
}

fn fixture() -> (tempfile::TempDir, QualityWorkspace) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, "VALUE = 1\n").unwrap();
    let workspace = Filesystem.load(&[path], true);
    (directory, workspace)
}

fn entries(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join(".mamba-quality-cache/v2"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect()
}

#[test]
fn concurrent_checks_share_one_writer_and_warm_check_reuses_complete_report() {
    let (directory, workspace) = fixture();
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Barrier::new(2));
    let first_workspace = workspace.clone();
    let second_workspace = workspace.clone();
    let reports = std::thread::scope(|scope| {
        let run = |workspace: &QualityWorkspace| {
            CachedEngine::new(ProbeEngine {
                calls: calls.clone(),
                gate: Some(gate.clone()),
                mutate: None,
                completion: CompletionStatus::Complete,
            })
            .check(workspace, &FixedProfile::locked(), QualityTool::Ruff)
        };
        let first = scope.spawn(move || run(&first_workspace));
        let second = scope.spawn(move || run(&second_workspace));
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        serde_json::to_value(&reports.0).unwrap(),
        serde_json::to_value(&reports.1).unwrap()
    );
    let cache = entries(directory.path());
    assert_eq!(cache.len(), 1);
    let envelope: Envelope = serde_json::from_slice(&fs::read(&cache[0]).unwrap()).unwrap();
    assert_eq!(envelope.schema, CACHE_SCHEMA);
    assert_eq!(envelope.report.completion, CompletionStatus::Complete);
    let warm = CachedEngine::new(ProbeEngine {
        calls: calls.clone(),
        gate: None,
        mutate: None,
        completion: CompletionStatus::Complete,
    })
    .check(&workspace, &FixedProfile::locked(), QualityTool::Ruff);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        serde_json::to_value(warm).unwrap(),
        serde_json::to_value(reports.0).unwrap()
    );
}

#[test]
fn dependency_change_during_check_does_not_publish_stale_result() {
    let (directory, workspace) = fixture();
    let dependency = directory.path().join("dependency.pyi");
    fs::write(&dependency, "VALUE: int\n").unwrap();
    let engine = CachedEngine::new(ProbeEngine {
        calls: Arc::new(AtomicUsize::new(0)),
        gate: None,
        mutate: Some(dependency.clone()),
        completion: CompletionStatus::Complete,
    });
    let report = engine.check(&workspace, &FixedProfile::locked(), QualityTool::Ruff);
    assert_eq!(report.completion, CompletionStatus::Complete);
    assert_eq!(fs::read_to_string(dependency).unwrap(), "VALUE: str\n");
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn incomplete_report_never_becomes_a_warm_cache_hit() {
    let (directory, workspace) = fixture();
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = CachedEngine::new(ProbeEngine {
        calls: calls.clone(),
        gate: None,
        mutate: None,
        completion: CompletionStatus::Incomplete,
    });
    for _ in 0..2 {
        let report = engine.check(&workspace, &FixedProfile::locked(), QualityTool::Ruff);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(entries(directory.path()).is_empty());
}

#[test]
fn subprocess_entry_claim_respects_live_writer_and_claims_after_release() {
    const CHILD: &str = "MAMBA_QUALITY_CACHE_LOCK_CHILD";
    if let Some(directory) = std::env::var_os(CHILD) {
        let expected = std::env::var("MAMBA_QUALITY_CACHE_LOCK_EXPECTED").unwrap() == "claimed";
        assert_eq!(
            Entry::claim(Path::new(&directory), "process-key").is_some(),
            expected
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(directory.path()).unwrap();
    let writer = Entry::claim(&path, "process-key").unwrap();
    let run = |expected: &str| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "infrastructure::cache::tests::admission::subprocess_entry_claim_respects_live_writer_and_claims_after_release", "--nocapture"])
            .env(CHILD, &path)
            .env("MAMBA_QUALITY_CACHE_LOCK_EXPECTED", expected)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    };
    run("busy");
    drop(writer);
    run("claimed");
}
