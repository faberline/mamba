use super::EmbeddedBindings;
use crate::{
    domain::source::{
        ProjectInfo, PythonVersion, QualityWorkspace, SourceEncoding, SourceSnapshot,
    },
    infrastructure::{embedded, project::ProjectIndex},
};
use mamba_language::application::quality_syntax::{self, python_ast::Expr, PythonTarget};
use ruff_text_size::Ranged;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs, path::Path};

fn embedded_modules() -> Vec<String> {
    fn walk(root: &Path, directory: &Path, names: &mut BTreeMap<String, ()>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, names);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "pyi") {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let Some(mut module) = relative.to_str().map(str::to_owned) else {
                continue;
            };
            module = module.trim_end_matches(".pyi").to_owned();
            if module == "__init__" {
                continue;
            }
            if let Some(package) = module.strip_suffix("/__init__") {
                module = package.to_owned();
            }
            names.insert(module.replace('/', "."), ());
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/typeshed/stdlib");
    let mut names = BTreeMap::new();
    walk(&root, &root, &mut names);
    names
        .into_keys()
        .filter(|name| crate::infrastructure::embedded::module(name).is_some())
        .collect()
}

fn binding_digest(bindings: &BTreeMap<String, Expr>) -> String {
    let mut digest = Sha256::new();
    for (name, expression) in bindings {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(expression.start().to_u32().to_be_bytes());
        digest.update(expression.end().to_u32().to_be_bytes());
        digest.update(format!("{expression:?}").as_bytes());
        digest.update([0xff]);
    }
    format!("{:x}", digest.finalize())
}

#[test]
fn freezes_uncached_embedded_source_binding_behavior() {
    let modules = embedded_modules();
    assert!(!modules.is_empty());
    let mut rows = Vec::new();
    for (version, target) in [
        ("3.12", PythonTarget::Py312),
        ("3.13", PythonTarget::Py313),
        ("3.14", PythonTarget::Py314),
    ] {
        for module in &modules {
            let source = crate::infrastructure::embedded::module(module)
                .expect("enumerated embedded module source");
            let source_sha256 = format!("{:x}", Sha256::digest(source.as_bytes()));
            let parsed = quality_syntax::parse(source, target, false);
            let row = match parsed {
                Ok(document) => {
                    let bindings = crate::infrastructure::pylint::source_bindings::collect(
                        &document.parsed.syntax().body,
                    );
                    json!({
                        "python": version,
                        "module": module,
                        "source_sha256": source_sha256,
                        "parse": "ok",
                        "binding_count": bindings.len(),
                        "bindings_sha256": binding_digest(&bindings),
                    })
                }
                Err(error) => json!({
                    "python": version,
                    "module": module,
                    "source_sha256": source_sha256,
                    "parse": "error",
                    "error_debug": format!("{error:?}"),
                }),
            };
            rows.push(row);
        }
    }
    let result = json!({
        "schema": "mamba.quality.pylint-embedded-source-bindings-legacy.v1",
        "modules": modules.len(),
        "targets": ["3.12", "3.13", "3.14"],
        "rows": rows,
    });
    if let Some(path) = env::var_os("MAMBA_QUALITY_PYLINT_EMBEDDED_BINDINGS_BASELINE_EVIDENCE") {
        fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
}

fn expression_snapshot(expression: &Expr) -> (u32, u32, String) {
    (
        expression.start().to_u32(),
        expression.end().to_u32(),
        format!("{expression:?}"),
    )
}

#[test]
fn cached_bindings_match_legacy_expressions_for_all_targets() {
    let mut compared = 0;
    let mut cached_rows = 0;
    let mut rows = Vec::new();
    for (version, target) in [
        ("3.12", PythonTarget::Py312),
        ("3.13", PythonTarget::Py313),
        ("3.14", PythonTarget::Py314),
    ] {
        for module in embedded_modules() {
            let source = embedded::module(&module).expect("enumerated embedded module source");
            let Ok(document) = quality_syntax::parse(source, target, false) else {
                let cache = EmbeddedBindings::default();
                assert_eq!(cache.binding(&module, source, target, "__missing__"), None);
                assert_eq!(cache.cached_modules(), 0, "{version} {module}");
                continue;
            };
            let legacy = crate::infrastructure::pylint::source_bindings::collect(
                &document.parsed.syntax().body,
            );
            let cache = EmbeddedBindings::default();
            let context =
                crate::infrastructure::pylint::resolution::Context::Module(module.clone());
            for (name, expected) in &legacy {
                let actual = cache.binding(&module, source, target, name);
                assert_eq!(
                    actual.as_ref().map(expression_snapshot),
                    Some(expression_snapshot(expected)),
                    "{version} {module}.{name}"
                );
                let actual_output =
                    actual.map(|value| (expression_snapshot(&value), context.clone()));
                assert!(
                    actual_output
                        .as_ref()
                        .is_some_and(|(expression, actual_context)| {
                            expression == &expression_snapshot(expected)
                                && actual_context == &context
                        }),
                    "{version} {module}.{name}"
                );
                compared += 1;
            }
            assert_eq!(
                cache.binding(&module, source, target, "__mamba_missing_binding__"),
                None,
                "{version} {module} missing lookup"
            );
            assert_eq!(
                cache.binding(&module, source, target, "__mamba_missing_binding__"),
                None,
                "{version} {module} repeated missing lookup"
            );
            let stats = cache.stats();
            assert_eq!(stats.parse_attempts, 1, "{version} {module}");
            assert_eq!(stats.insertions, 1, "{version} {module}");
            assert_eq!(stats.parse_failures, 0, "{version} {module}");
            assert_eq!(cache.cached_modules(), 1, "{version} {module}");
            rows.push(json!({
                "python": version,
                "module": module,
                "binding_count": legacy.len(),
                "bindings_sha256": binding_digest(&legacy),
                "cache_hits": stats.hits,
                "cache_misses": stats.misses,
                "parse_attempts": stats.parse_attempts,
                "insertions": stats.insertions,
                "context": "Module",
            }));
            cached_rows += 1;
        }
    }
    assert_eq!(compared, 9_957);
    assert_eq!(cached_rows, 2_256);
    if let Some(path) = env::var_os("MAMBA_QUALITY_PYLINT_EMBEDDED_BINDINGS_EVIDENCE") {
        let evidence = json!({
            "schema": "mamba.quality.pylint-embedded-source-bindings-cache.v1",
            "modules": 752,
            "targets": ["3.12", "3.13", "3.14"],
            "bindings_compared": compared,
            "cache_rows": cached_rows,
            "rows": rows,
        });
        fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn cache_is_bounded_and_preserves_recovered_bindings() {
    let names = embedded_modules();
    let cache = EmbeddedBindings::default();
    let mut completed = 0;
    for module in names.iter().take(EmbeddedBindings::test_module_limit() + 1) {
        let source = embedded::module(module).expect("known embedded source");
        let _ = cache.binding(module, source, PythonTarget::Py312, "__probe__");
        completed += 1;
    }
    assert_eq!(completed, EmbeddedBindings::test_module_limit() + 1);
    assert_eq!(
        cache.cached_modules(),
        EmbeddedBindings::test_module_limit()
    );
    assert!(cache.cached_bindings() <= EmbeddedBindings::test_binding_limit());

    let issue_source = "x = 1\ndef broken(:\n";
    let document = quality_syntax::parse(issue_source, PythonTarget::Py312, false).unwrap();
    assert!(!document.issues.is_empty());
    let expected =
        crate::infrastructure::pylint::source_bindings::collect(&document.parsed.syntax().body)
            .get("x")
            .map(expression_snapshot);
    let issue_cache = EmbeddedBindings::default();
    let before = issue_cache.stats();
    let actual = issue_cache
        .binding(
            "__syntax_issue_embedded_test__",
            issue_source,
            PythonTarget::Py312,
            "x",
        )
        .as_ref()
        .map(expression_snapshot);
    let after_first = issue_cache.stats();
    assert_eq!(actual, expected);
    assert_eq!(after_first.parse_attempts - before.parse_attempts, 1);
    assert_eq!(after_first.parse_failures, before.parse_failures);
    assert_eq!(after_first.insertions - before.insertions, 1);
    assert_eq!(
        issue_cache
            .binding(
                "__syntax_issue_embedded_test__",
                issue_source,
                PythonTarget::Py312,
                "x",
            )
            .as_ref()
            .map(expression_snapshot),
        expected
    );
    let after = issue_cache.stats();
    assert_eq!(after.parse_attempts, after_first.parse_attempts);
    assert_eq!(after.insertions, after_first.insertions);
    assert_eq!(
        cache.cached_modules(),
        EmbeddedBindings::test_module_limit()
    );
}

fn project_modules(
    root: &Path,
    module: &str,
    source: &str,
    is_stub: bool,
) -> super::super::Modules {
    let path = root.join(format!("{module}.pyi"));
    let bytes = source.as_bytes().to_vec();
    let workspace = QualityWorkspace {
        projects: vec![ProjectInfo {
            root: root.to_path_buf(),
            python_version: PythonVersion::Py312,
            requires_python: None,
            metadata_sha256: None,
            dependencies: Vec::new(),
        }],
        sources: vec![SourceSnapshot {
            path,
            project: 0,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes,
            text: source.to_owned(),
            encoding: SourceEncoding::Utf8 { bom: false },
            decoding_issue: None,
            is_stub,
        }],
        ..QualityWorkspace::default()
    };
    let index = ProjectIndex::new(&workspace);
    super::super::Modules::new(&index, &workspace, 0)
}

#[test]
fn project_sources_that_shadow_stub_names_are_not_cached_as_embedded() {
    let root = tempfile::tempdir().unwrap();
    let project_text = "def loads(value):\n    return value\n";
    let modules = project_modules(root.path(), "json", project_text, true);
    assert_eq!(modules.source(0, "json"), Some(project_text));
    assert_eq!(modules.embedded_binding(0, "json", "loads"), None);
    assert_eq!(modules.embedded_bindings.cached_modules(), 0);

    let native_stub = embedded::module("os").expect("embedded os stub");
    let native_root = tempfile::tempdir().unwrap();
    let shadow = project_modules(native_root.path(), "os", native_stub, true);
    assert_eq!(shadow.source(0, "os"), Some(native_stub));
    assert_eq!(shadow.embedded_binding(0, "os", "path"), None);
    assert_eq!(shadow.embedded_bindings.cached_modules(), 0);
}
