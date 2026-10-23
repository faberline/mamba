//! Resolve primitive aliases with a dependency queue, without repeated full scans.
use super::*;
use std::collections::{BTreeSet, VecDeque};

#[derive(Default)]
struct CallableEnvironment {
    snapshot: Option<BTreeMap<String, Type>>,
}

impl CallableEnvironment {
    fn get(&mut self, scopes: &Scopes) -> &BTreeMap<String, Type> {
        self.snapshot.get_or_insert_with(|| scopes.environment())
    }

    fn invalidate_for_type_change(&mut self, before: Option<Type>, after: Option<Type>) {
        if before != after {
            self.snapshot = None;
        }
    }
}

impl Scopes {
    pub(super) fn infer_declarations(&mut self, body: &[Stmt]) {
        let assignments: Vec<_> = body.iter().filter_map(Stmt::as_assign_stmt).collect();
        let mut dependents: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (index, assignment) in assignments.iter().enumerate() {
            let mut names = BTreeSet::new();
            dependencies(&assignment.value, &mut names);
            for name in names {
                dependents.entry(name).or_default().push(index);
            }
        }
        let mut environment = self.environment();
        // This separate snapshot retains fresh lexical lookup, including unknown shadows.
        let mut callable_environment = CallableEnvironment::default();
        let recursive = super::inference_cycles::recursive_assignments(&assignments);
        let mut pending: VecDeque<_> = (0..assignments.len()).collect();
        let mut queued = vec![true; assignments.len()];
        while let Some(index) = pending.pop_front() {
            queued[index] = false;
            let assignment = assignments[index];
            let ty = self
                .recover_constructor_assignment(&assignment.value, &environment)
                .or_else(|| {
                    self.context
                        .infer_with_calls(&assignment.value, &environment, &|call| {
                            self.expression_type(call, &environment)
                        })
                });
            let callable = self
                .callable_expression_in(&assignment.value, callable_environment.get(self))
                .map(|signature| Arc::new(signature.anonymous()));
            if ty.is_none() && callable.is_none() {
                continue;
            }
            for target in &assignment.targets {
                let Expr::Name(name) = target else {
                    continue;
                };
                // Do not replace the inferred local discard name's dynamic Any
                // with a type guessed from one assignment in this prepass.
                if name.id.as_str() == "_"
                    && self.current().kind == Kind::Function
                    && !self.current().redirects.contains_key(name.id.as_str())
                    && self
                        .current()
                        .bindings
                        .get(name.id.as_str())
                        .is_some_and(|binding| binding.infer_from_assignment)
                {
                    continue;
                }
                let binding = self.binding_mut(name.id.as_str()).unwrap();
                if !binding.infer_from_assignment
                    || binding.first_definition != name.start()
                    || (recursive[index] && binding.ty.is_some())
                    || (binding.ty == ty && binding.callable == callable)
                {
                    continue;
                }
                callable_environment.invalidate_for_type_change(binding.ty, ty);
                binding.ty = ty;
                binding.annotation_form = None;
                binding.annotation_module = None;
                binding.callable = callable.clone();
                if let Some(ty) = ty {
                    environment.insert(name.id.to_string(), ty);
                }
                if let Some(dependents) = dependents.get(name.id.as_str()) {
                    for &dependent in dependents {
                        if !queued[dependent] {
                            queued[dependent] = true;
                            pending.push_back(dependent);
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn dependencies<'a>(expression: &'a Expr, names: &mut BTreeSet<&'a str>) {
    match expression {
        Expr::Name(name) => {
            names.insert(name.id.as_str());
        }
        Expr::BinOp(binary) => {
            dependencies(&binary.left, names);
            dependencies(&binary.right, names);
        }
        Expr::Call(call) => {
            dependencies(&call.func, names);
            for value in &call.arguments.args {
                dependencies(value, names);
            }
            for keyword in &call.arguments.keywords {
                dependencies(&keyword.value, names);
            }
        }
        Expr::Attribute(attribute) => dependencies(&attribute.value, names),
        Expr::List(list) => {
            for item in &list.elts {
                dependencies(item, names);
            }
        }
        Expr::Set(set) => {
            for item in &set.elts {
                dependencies(item, names);
            }
        }
        Expr::Tuple(tuple) => {
            for item in &tuple.elts {
                dependencies(item, names);
            }
        }
        Expr::Dict(dictionary) => {
            for item in &dictionary.items {
                if let Some(key) = &item.key {
                    dependencies(key, names);
                }
                dependencies(&item.value, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamba_language::application::quality_syntax::{self, PythonTarget};

    #[test]
    fn local_callable_environment_tracks_types_and_unknown_shadows() {
        let document = quality_syntax::parse("shadow = 1\n", PythonTarget::Py312, false).unwrap();
        assert!(document.is_valid());
        let mut scopes = Scopes::new(
            &document.parsed.syntax().body,
            std::rc::Rc::new(types::Context::default()),
            "0:probe".into(),
        );
        assert_eq!(scopes.environment().get("shadow"), Some(&Type::Int));
        scopes.enter(
            Kind::Function,
            &[],
            BTreeMap::from([("shadow".into(), None)]),
            types::Variables::new(),
        );
        let mut cached = CallableEnvironment::default();
        assert_eq!(cached.get(&scopes), &scopes.environment());
        assert!(!cached.get(&scopes).contains_key("shadow"));

        for after in [Some(Type::Int), None, Some(Type::Str), Some(Type::Bool)] {
            let binding = scopes.binding_mut("shadow").unwrap();
            cached.invalidate_for_type_change(binding.ty, after);
            binding.ty = after;
            assert!(cached.snapshot.is_none());
            assert_eq!(cached.get(&scopes), &scopes.environment());
            assert_eq!(cached.get(&scopes).get("shadow").copied(), after);
        }

        let binding = scopes.binding_mut("shadow").unwrap();
        binding.callable = Some(Arc::new(Signature::empty("callback".into(), Type::Int)));
        cached.invalidate_for_type_change(binding.ty, binding.ty);
        assert!(cached.snapshot.is_some());
        assert_eq!(cached.get(&scopes), &scopes.environment());
        assert_eq!(
            scopes.callable("shadow").unwrap().name.as_deref(),
            Some("callback")
        );
    }
}
