use super::{sources_with_catalog, Catalog};
use std::{collections::BTreeSet, fs, path::PathBuf};

// This is the frozen, uncached lookup. It checks the catalog's result and order.
fn uncached_sources(
    roots: &[PathBuf],
    name: &str,
    errors: &mut BTreeSet<String>,
) -> Vec<(PathBuf, String)> {
    let parts: Vec<_> = name.split('.').filter(|part| !part.is_empty()).collect();
    for root in roots {
        let mut current = root.clone();
        let mut module = String::new();
        let mut selected = Vec::new();
        for (offset, part) in parts.iter().enumerate() {
            current.push(part);
            if offset > 0 {
                module.push('.');
            }
            module.push_str(part);
            let candidates = [
                current.join("__init__.pyi"),
                current.join("__init__.py"),
                current.with_extension("pyi"),
                current.with_extension("py"),
            ];
            if let Some(path) = candidates.iter().find(|path| path.is_file()) {
                match path.canonicalize() {
                    Ok(path) => selected.push((path, module.clone())),
                    Err(error) => {
                        errors.insert(format!("{}: {error}", path.display()));
                    }
                }
                if path
                    .file_name()
                    .is_some_and(|name| name != "__init__.py" && name != "__init__.pyi")
                {
                    break;
                }
            } else if !current.is_dir() {
                break;
            }
        }
        if !selected.is_empty() {
            return selected;
        }
    }
    Vec::new()
}

fn compare(roots: &[PathBuf], names: &[&str], catalog: &mut Catalog) {
    for name in names {
        let mut expected_errors = BTreeSet::new();
        let mut actual_errors = BTreeSet::new();
        let expected = uncached_sources(roots, name, &mut expected_errors);
        let actual = sources_with_catalog(roots, name, catalog, &mut actual_errors);
        assert_eq!(actual, expected, "lookup {name}");
        let mut direct_errors = BTreeSet::new();
        assert_eq!(
            super::sources(roots, name, &mut direct_errors),
            expected,
            "single query {name}"
        );
        assert_eq!(direct_errors, expected_errors, "single query errors {name}");
        assert_eq!(actual_errors, expected_errors, "errors for {name}");
    }
}

#[test]
fn catalog_preserves_package_module_stub_namespace_and_root_order() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(first.join("pkg")).unwrap();
    fs::create_dir_all(first.join("ns")).unwrap();
    fs::create_dir_all(&second).unwrap();
    for path in [
        first.join("pkg/__init__.py"),
        first.join("pkg/__init__.pyi"),
        first.join("pkg/sub.py"),
        first.join("pkg/sub.pyi"),
        first.join("ns/leaf.py"),
        first.join("ordered.py"),
        second.join("ordered.pyi"),
        second.join("fallback.pyi"),
    ] {
        fs::write(path, b"pass\n").unwrap();
    }
    let roots = [first, second];
    let mut catalog = Catalog::default();
    compare(
        &roots,
        &[
            "pkg",
            "pkg.sub",
            "ns",
            "ns.leaf",
            "ordered",
            "fallback",
            "absent.deep",
        ],
        &mut catalog,
    );
    assert!(catalog.avoided_probes() > 0);
}

#[test]
fn repeated_missing_imports_avoid_live_probes() {
    let temp = tempfile::tempdir().unwrap();
    let roots = [temp.path().to_path_buf()];
    let mut catalog = Catalog::default();
    for _ in 0..20 {
        compare(&roots, &["missing", "missing.child"], &mut catalog);
    }
    assert!(catalog.avoided_probes() >= 20);
}

#[test]
fn many_missing_names_across_roots_skip_package_and_module_probes() {
    let temp = tempfile::tempdir().unwrap();
    let roots: Vec<_> = (0..24)
        .map(|index| temp.path().join(format!("root_{index}")))
        .collect();
    for root in &roots {
        fs::create_dir(root).unwrap();
    }
    let names: Vec<_> = (0..32).map(|index| format!("missing_{index}")).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    let mut catalog = Catalog::default();
    compare(&roots, &names, &mut catalog);
    assert_eq!(catalog.avoided_probes(), roots.len() * names.len() * 5);
}

#[cfg(unix)]
#[test]
fn symlinks_keep_canonical_paths_and_stub_preference() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("real_pkg")).unwrap();
    fs::write(root.join("real_pkg/__init__.pyi"), b"pass\n").unwrap();
    fs::write(root.join("real.py"), b"pass\n").unwrap();
    symlink("real_pkg", root.join("alias_pkg")).unwrap();
    symlink("real.py", root.join("alias.py")).unwrap();
    let roots = [root.to_path_buf()];
    let mut catalog = Catalog::default();
    compare(&roots, &["alias_pkg", "alias"], &mut catalog);
}

#[test]
fn enumeration_failure_and_limits_use_live_probes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    fs::write(root.join("present.py"), b"pass\n").unwrap();
    let mut limited = Catalog::with_limits(0, 0);
    compare(
        std::slice::from_ref(&root),
        &["present", "missing"],
        &mut limited,
    );
    assert_eq!(limited.avoided_probes(), 0);

    fs::write(root.join("other.py"), b"pass\n").unwrap();
    let mut too_many_names = Catalog::with_limits(1, 1);
    compare(
        std::slice::from_ref(&root),
        &["present", "other", "missing"],
        &mut too_many_names,
    );
    assert_eq!(too_many_names.avoided_probes(), 0);

    let missing_root = root.join("does_not_exist");
    let mut failed = Catalog::default();
    compare(&[missing_root], &["anything"], &mut failed);
    assert_eq!(failed.avoided_probes(), 0);
}
