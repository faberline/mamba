//! Resolver precedence and declaration shape checks; no new original diagnostics.
use super::*;
use crate::infrastructure::mypy::{
    modules::{Exports, Modules},
    types::Type,
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

fn lookup(target: PythonTarget, imported: bool, local_parent: bool) -> Lookup {
    let context = Rc::new(Context::default());
    context.set_module_target("0:main".into(), target);
    let mut info = vec![(0, "main".into(), false)];
    let mut names = BTreeMap::from([((0, "main".into()), 0)]);
    let mut namespaces = BTreeSet::from([(0, "main".into())]);
    if local_parent {
        info.push((0, "importlib".into(), true));
        names.insert((0, "importlib".into()), 1);
        namespaces.insert((0, "importlib".into()));
    }
    let count = info.len();
    let modules = Rc::new(Modules {
        context,
        names,
        info,
        shadow_namespaces: namespaces.clone(),
        embedded_sources: vec![false; count],
        namespaces,
        exports: RefCell::new(vec![Exports::new(); count]),
        export_states: RefCell::new(vec![Some(Default::default()); count]),
        export_gaps: RefCell::new(vec![BTreeSet::new(); count]),
        export_uses: RefCell::new(vec![BTreeSet::new(); count]),
        early_issues: RefCell::new(vec![None; count]),
        reexports: vec![BTreeMap::new(); count],
        dependency_roots: vec![Vec::new()],
        untyped: BTreeSet::new(),
        imported: if imported {
            BTreeSet::from([(0, "importlib.util".into())])
        } else {
            BTreeSet::new()
        },
    });
    Modules::lookup(&modules, 0)
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn child_visibility_preserves_parent_and_local_namespace() {
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        assert!(lookup(target, false, false)
            .member("importlib", "util")
            .is_none());
        assert_eq!(
            lookup(target, true, false)
                .member("importlib", "util")
                .unwrap()
                .module
                .as_deref(),
            Some("importlib.util")
        );
        assert!(lookup(target, true, true)
            .member("importlib", "util")
            .is_none());
        let lookup = lookup(target, true, false);
        assert!(lookup
            .member("importlib.util", "definitely_missing_mamba_gap_member")
            .is_none());
        for name in ["spec_from_file_location", "module_from_spec"] {
            let export = lookup.member("importlib.util", name).unwrap();
            let callable = export.callable.unwrap();
            assert!(export.explicit);
            assert_eq!(
                callable.definition_module.as_deref(),
                Some("importlib.util")
            );
            assert!(callable.native_symbol.is_none());
            assert!(callable
                .parameters
                .iter()
                .all(|parameter| parameter.ty.is_some_and(|ty| ty != Type::Any)));
            assert!(callable.returns.is_some_and(|ty| ty != Type::Any));
        }
    }
}

#[test]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn unobserved_platform_has_no_child_declarations() {
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        assert!(!lookup(target, true, false).typed_child_module_exists("importlib.util"));
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn direct_child_lookup_rejects_local_ancestors_and_namespaces() {
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        let local_parent = lookup(target, true, true);
        assert!(!local_parent.typed_child_module_exists("importlib.util"));
        assert!(local_parent
            .member("importlib.util", "spec_from_file_location")
            .is_none());
        assert!(local_parent
            .typed_child_member("importlib.util", "module_from_spec")
            .is_none());
        for namespace in ["importlib", "importlib.util"] {
            let mut local_namespace = lookup(target, true, false);
            let modules = Rc::get_mut(&mut local_namespace.modules).unwrap();
            modules.namespaces.insert((0, namespace.into()));
            modules.shadow_namespaces.insert((0, namespace.into()));
            assert!(!local_namespace.typed_child_module_exists("importlib.util"));
            assert!(local_namespace
                .member("importlib.util", "spec_from_file_location")
                .is_none());
            assert!(local_namespace
                .typed_child_member("importlib.util", "module_from_spec")
                .is_none());
        }
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn aliased_child_binding_keeps_typed_module_identity() {
    use crate::infrastructure::mypy::scopes::Scopes;
    use mamba_language::application::quality_syntax;
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        let lookup = lookup(target, true, false);
        let source = "import importlib.util as util\n";
        let document = quality_syntax::parse(source, target, false).unwrap();
        assert!(document.is_valid());
        let body = &document.parsed.syntax().body;
        let mut scopes = Scopes::new(body, lookup.modules.context.clone(), "0:main".into());
        scopes.attach_modules(lookup.clone(), body);
        let exports = scopes.exports();
        assert_eq!(exports["util"].module.as_deref(), Some("importlib.util"));
        assert_ne!(exports["util"].ty, Some(Type::Any));
        let member = lookup
            .member(
                exports["util"].module.as_deref().unwrap(),
                "spec_from_file_location",
            )
            .unwrap();
        assert!(member.callable.is_some());
        assert!(member.ty.is_some_and(|ty| ty != Type::Any));
    }
}
