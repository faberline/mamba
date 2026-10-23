//! Candidate validation faults must keep the original source unchanged.
use super::*;

#[derive(Clone, Copy)]
enum Fault {
    Syntax,
    Structure,
    Convergence,
}

struct FaultEngine(Fault);

impl QualityEngine for FaultEngine {
    fn check(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        Engine::new(None, false).check(workspace, profile, tool)
    }

    fn transform(
        &self,
        workspace: &QualityWorkspace,
        _: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult {
        let candidates = if tool == QualityTool::Black {
            workspace
                .sources
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    let candidate = match self.0 {
                        Fault::Syntax => "VALUE = (\n".into(),
                        Fault::Structure => "VALUE = 2\n".into(),
                        Fault::Convergence if source.text.contains("VALUE=1") => {
                            "VALUE = 1\n".into()
                        }
                        Fault::Convergence => "VALUE=1\n".into(),
                    };
                    (index, candidate)
                })
                .collect()
        } else {
            Vec::new()
        };
        TransformResult {
            completion: CompletionStatus::Complete,
            candidates,
            issues: Vec::new(),
            remaining: Vec::new(),
        }
    }

    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        equivalent: bool,
    ) -> Result<(), QualityIssue> {
        NativeEngine.validate(original, candidate, workspace, equivalent)
    }
}

#[test]
fn invalid_syntax_changed_structure_and_nonconvergence_never_publish() {
    for version in ["3.12", "3.13", "3.14"] {
        for (fault, code) in [
            (Fault::Syntax, "candidate-syntax"),
            (Fault::Structure, "candidate-structure"),
            (Fault::Convergence, "fix-convergence"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            fs::write(
                directory.path().join("pyproject.toml"),
                format!("[project]\nrequires-python = '>={version}'\n"),
            )
            .unwrap();
            let path = directory.path().join("main.py");
            fs::write(&path, "VALUE=1\n").unwrap();
            let report = execute(
                &QualityRequest::new(
                    QualityOperation::Format {
                        check: false,
                        diff: false,
                    },
                    vec![path.clone()],
                ),
                &Filesystem,
                &FaultEngine(fault),
            );
            assert_eq!(report.exit_code, 2);
            assert_eq!(report.files.written, 0);
            assert!(
                report.issues.iter().any(|issue| issue.code == code),
                "{:?}",
                report.issues
            );
            assert!(report.changes.iter().all(|change| !change.applied));
            assert_eq!(fs::read_to_string(path).unwrap(), "VALUE=1\n");
        }
    }
}

#[test]
fn black_structure_guard_checks_stub_and_versioned_interpolation() {
    for (version, filename, before, after) in [
        (
            "3.12",
            "main.py",
            "type Alias[T] = list[T]\n",
            "type Alias[T] = set[T]\n",
        ),
        (
            "3.13",
            "main.pyi",
            "def fetch[T](value: T) -> T: ...\n",
            "def fetch[T](value: T) -> int: ...\n",
        ),
        (
            "3.14",
            "main.py",
            "VALUE = t'{name}'\n",
            "VALUE = t'{other}'\n",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("pyproject.toml"),
            format!("[project]\nrequires-python = '>={version}'\n"),
        )
        .unwrap();
        let path = directory.path().join(filename);
        fs::write(&path, before).unwrap();
        let workspace = Filesystem.load(&[path], true);
        assert!(workspace.issues.is_empty(), "{:?}", workspace.issues);
        let error = NativeEngine
            .validate(&workspace.sources[0], after, &workspace, true)
            .unwrap_err();
        assert_eq!(error.code, "candidate-structure");
    }
}
