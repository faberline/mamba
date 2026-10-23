//! Real Context inference and owned-call guards. No original-tool oracle is needed.
use super::{Context, Node, ObservedExpressionTypes, Type};
use crate::infrastructure::mypy::scopes::Scopes;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{Expr, ExprCall, Stmt},
    PythonTarget,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
    sync::{Arc, Mutex},
};

fn with_expression(source: &str, test: impl FnOnce(&Expr)) {
    let document = quality_syntax::parse(source, PythonTarget::Py314, false)
        .expect("Rust parser accepts the fixture");
    assert!(document.is_valid(), "fixture must have no syntax issues");
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("fixture must contain one expression statement");
    };
    test(&statement.value);
}

fn call(expression: &Expr) -> &ExprCall {
    let Expr::Call(call) = expression else {
        panic!("call fixture required")
    };
    call
}

fn key(expression: &Expr) -> usize {
    expression as *const Expr as usize
}

fn observations(
    entries: impl IntoIterator<Item = (usize, Option<Type>)>,
) -> ObservedExpressionTypes {
    Arc::new(Mutex::new(entries.into_iter().collect::<HashMap<_, _>>()))
}

fn current(context: &Context) -> Option<ObservedExpressionTypes> {
    context.observed_expression_types.borrow().clone()
}

fn assert_current(context: &Context, expected: Option<&ObservedExpressionTypes>) {
    match (current(context), expected) {
        (None, None) => {}
        (Some(actual), Some(expected)) => assert!(
            Arc::ptr_eq(&actual, expected),
            "restore the exact map owner"
        ),
        _ => panic!("observation slot state differs"),
    }
}

fn call_keys(call: &ExprCall) -> Vec<usize> {
    std::iter::once(key(&call.func))
        .chain(call.arguments.args.iter().map(key))
        .chain(
            call.arguments
                .keywords
                .iter()
                .map(|keyword| key(&keyword.value)),
        )
        .collect()
}

fn move_owned<T>(value: T) -> T {
    value
}

fn assert_removed(context: &Context, copied: &[usize]) {
    let values = current(context).expect("outer map remains active");
    let values = values.lock().expect("map lock");
    for address in copied {
        assert!(
            !values.contains_key(address),
            "copied address must not escape its owner"
        );
    }
}

#[test]
fn context_default_is_send_and_uses_the_ordinary_callback() {
    fn assert_send<T: Send>() {}
    assert_send::<Context>();
    let context = Context::default();
    assert_current(&context, None);
    with_expression("missing\n", |expression| {
        let calls = Cell::new(0);
        assert_eq!(
            context.infer_with_calls(expression, &BTreeMap::new(), &|_| {
                calls.set(calls.get() + 1);
                None
            }),
            None
        );
        assert_eq!(calls.get(), 1);
        assert_current(&context, None);
    });
    with_expression("b'fixture'\n", |expression| {
        assert_eq!(
            context.infer_with_calls(expression, &BTreeMap::new(), &|_| panic!(
                "literal inference must not call the fallback"
            )),
            Some(Type::Bytes)
        );
    });
}

#[test]
fn recorded_none_prevents_fallback_and_a_genuine_miss_uses_it() {
    let context = Context::default();
    with_expression("value\n", |expression| {
        let saved = observations([(key(expression), None)]);
        let calls = Cell::new(0);
        let callback = |_: &Expr| {
            calls.set(calls.get() + 1);
            Some(Type::Bytes)
        };
        assert_eq!(
            context.infer_with_observed(expression, &BTreeMap::new(), &callback, &saved),
            None
        );
        assert_eq!(calls.get(), 0);
        assert_current(&context, None);
        let empty = observations([]);
        assert_eq!(
            context.infer_with_observed(expression, &BTreeMap::new(), &callback, &empty),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 1);
        assert_current(&context, None);
        assert_eq!(
            context.infer_with_calls(expression, &BTreeMap::new(), &callback),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 2);
    });
}

#[test]
fn parent_composition_does_not_infer_its_observed_child_again() {
    let context = Context::default();
    with_expression("(saved, missing)\n", |expression| {
        let Expr::Tuple(tuple) = expression else {
            panic!("tuple fixture required")
        };
        let saved = observations([(key(&tuple.elts[0]), Some(Type::Bytes))]);
        let visited = RefCell::new(Vec::new());
        let callback = |value: &Expr| {
            visited.borrow_mut().push(key(value));
            if key(value) == key(&tuple.elts[0]) {
                Some(Type::Bytes)
            } else {
                Some(Type::Str)
            }
        };
        let actual = context
            .infer_with_observed(expression, &BTreeMap::new(), &callback, &saved)
            .expect("tuple type");
        let node = context.node(actual).expect("tuple node");
        let Node::Tuple(items) = node.as_ref() else {
            panic!("tuple result required")
        };
        assert_eq!(items.as_slice(), [Type::Bytes, Type::Str]);
        assert_eq!(visited.borrow().as_slice(), [key(&tuple.elts[1])]);
        visited.borrow_mut().clear();
        assert_eq!(
            context.infer_with_calls(expression, &BTreeMap::new(), &callback),
            Some(actual)
        );
        assert_eq!(
            visited.borrow().as_slice(),
            [key(&tuple.elts[0]), key(&tuple.elts[1])]
        );
    });
}

#[test]
fn observation_inference_restores_prior_owner_on_nested_none_and_panic() {
    let context = Context::default();
    with_expression("probe\n", |expression| {
        let prior = observations([]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let outer = observations([]);
        let inner = observations([(key(expression), Some(Type::Bytes))]);
        let calls = Cell::new(0);
        let result = context.infer_with_observed(
            expression,
            &BTreeMap::new(),
            &|_| {
                calls.set(calls.get() + 1);
                assert_current(&context, Some(&outer));
                assert_eq!(
                    context.infer_with_observed(
                        expression,
                        &BTreeMap::new(),
                        &|_| panic!("observed child must not call fallback"),
                        &inner
                    ),
                    Some(Type::Bytes)
                );
                assert_current(&context, Some(&outer));
                None
            },
            &outer,
        );
        assert_eq!(result, None);
        assert_eq!(calls.get(), 1);
        assert_current(&context, Some(&prior));
        let failure = catch_unwind(AssertUnwindSafe(|| {
            context.infer_with_observed(
                expression,
                &BTreeMap::new(),
                &|_| {
                    assert_current(&context, Some(&outer));
                    panic!("controlled observation inference panic");
                },
                &outer,
            )
        }));
        assert!(failure.is_err());
        assert_current(&context, Some(&prior));
        assert!(
            context.observed_expression_types.try_borrow_mut().is_ok(),
            "no slot borrow survives unwinding"
        );
    });
}

#[test]
fn fresh_local_inference_suspends_a_recorded_unknown_and_restores_it() {
    let context = Context::default();
    with_expression("parameter\n", |expression| {
        let prior = observations([(key(expression), None)]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let environment = BTreeMap::from([("parameter".into(), Type::Bytes)]);
        let calls = Cell::new(0);
        let callback = |_: &Expr| {
            calls.set(calls.get() + 1);
            Some(Type::Str)
        };
        assert_eq!(
            context.infer_with_calls(expression, &environment, &callback),
            None
        );
        assert_eq!(
            context.without_observed(|| {
                assert_current(&context, None);
                context.infer_with_calls(expression, &environment, &callback)
            }),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 0);
        assert_current(&context, Some(&prior));
        assert_eq!(
            context.infer_with_calls(expression, &environment, &callback),
            None
        );
        assert_eq!(
            context.without_observed(|| context.infer_with_calls(
                expression,
                &BTreeMap::new(),
                &callback
            )),
            Some(Type::Str)
        );
        assert_eq!(calls.get(), 1);
        assert_current(&context, Some(&prior));
    });
}

#[test]
fn fresh_local_suspension_restores_on_none_nested_scope_and_panic() {
    let context = Context::default();
    with_expression("parameter\n", |expression| {
        let prior = observations([(key(expression), Some(Type::Bytes))]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let temporary = observations([(key(expression), Some(Type::Str))]);
        let calls = Cell::new(0);
        let result: Option<Type> = context.without_observed(|| {
            assert_current(&context, None);
            assert_eq!(
                context.without_observed(|| context.infer_with_calls(
                    expression,
                    &BTreeMap::new(),
                    &|_| {
                        calls.set(calls.get() + 1);
                        None
                    }
                )),
                None
            );
            assert_current(&context, None);
            assert_eq!(
                context.infer_with_observed(
                    expression,
                    &BTreeMap::new(),
                    &|_| panic!("known nested value"),
                    &temporary
                ),
                Some(Type::Str)
            );
            assert_current(&context, None);
            None
        });
        assert_eq!(result, None);
        assert_eq!(calls.get(), 1);
        assert_current(&context, Some(&prior));
        let failure = catch_unwind(AssertUnwindSafe(|| {
            context.without_observed(|| {
                assert_current(&context, None);
                // Exercise real slot mutation without retaining a borrow into Drop.
                context
                    .observed_expression_types
                    .replace(Some(temporary.clone()));
                panic!("controlled fresh-local panic");
            })
        }));
        assert!(failure.is_err());
        assert_current(&context, Some(&prior));
        assert!(context.observed_expression_types.try_borrow_mut().is_ok());
    });
}

#[test]
fn fresh_local_inactive_path_does_not_create_a_map() {
    let context = Context::default();
    with_expression("probe\n", |expression| {
        let calls = Cell::new(0);
        assert_eq!(
            context.without_observed(|| context.infer_with_calls(
                expression,
                &BTreeMap::new(),
                &|_| {
                    calls.set(calls.get() + 1);
                    Some(Type::Int)
                }
            )),
            Some(Type::Int)
        );
        assert_eq!(calls.get(), 1);
        assert_current(&context, None);
    });
}

#[test]
fn expanded_call_keeps_allocated_addresses_and_checked_optional_types() {
    with_expression(
        "list(*(saved, unknown), untouched, keyword=keyword_value)\n",
        |expression| {
            let context = Rc::new(Context::default());
            let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
            let signature = context
                .collection_constructor("list", PythonTarget::Py314)
                .expect("real builtin signature");
            let canonical = context.callable_type(signature.clone());
            let original = call(expression);
            let Expr::Starred(starred) = &original.arguments.args[0] else {
                panic!("starred fixture required")
            };
            let Expr::Tuple(list) = starred.value.as_ref() else {
                panic!("list fixture required")
            };
            let prior = observations([
                (key(&original.func), Some(canonical)),
                (key(&list.elts[0]), Some(Type::Bytes)),
                (key(&list.elts[1]), None),
                (key(&original.arguments.keywords[0].value), None),
            ]);
            context
                .observed_expression_types
                .replace(Some(prior.clone()));
            let expanded = scopes
                .collection_expanded_call(original, &signature)
                .expect("literal star expands");
            let addresses = call_keys(&expanded);
            let expanded = move_owned(expanded);
            assert_eq!(
                call_keys(&expanded),
                addresses,
                "moving the owner must preserve Box and ThinVec element addresses"
            );
            let copies = current(&context).expect("copied map");
            assert!(!Arc::ptr_eq(&copies, &prior));
            for original_address in call_keys(original) {
                assert!(!addresses.contains(&original_address));
            }
            assert_eq!(expanded.arguments.args.len(), 3);
            let calls = Cell::new(0);
            let callback = |_: &Expr| {
                calls.set(calls.get() + 1);
                Some(Type::Int)
            };
            assert_eq!(
                context.infer_with_calls(&expanded.func, &BTreeMap::new(), &callback),
                Some(canonical)
            );
            assert_eq!(
                context.infer_with_calls(&expanded.arguments.args[0], &BTreeMap::new(), &callback),
                Some(Type::Bytes)
            );
            assert_eq!(
                context.infer_with_calls(&expanded.arguments.args[1], &BTreeMap::new(), &callback),
                None
            );
            assert_eq!(
                context.infer_with_calls(
                    &expanded.arguments.keywords[0].value,
                    &BTreeMap::new(),
                    &callback
                ),
                None
            );
            assert_eq!(
                calls.get(),
                0,
                "checked callee, positional, and keyword records do not fall back"
            );
            assert_eq!(
                context.infer_with_calls(&expanded.arguments.args[2], &BTreeMap::new(), &callback),
                Some(Type::Int)
            );
            assert_eq!(
                calls.get(),
                1,
                "a missing original record still uses ordinary inference"
            );
            assert_current(&context, Some(&copies));
            drop(expanded);
            assert_current(&context, Some(&prior));
            assert_removed(&context, &addresses);
            assert_eq!(
                context.infer_with_calls(&list.elts[0], &BTreeMap::new(), &callback),
                Some(Type::Bytes)
            );
            assert_eq!(
                calls.get(),
                1,
                "outer original records survive copied-call Drop"
            );
            assert_eq!(context.callable_signature(canonical), Some(signature));
        },
    );
}

#[test]
fn owned_call_guard_removes_keys_while_the_copied_ast_is_still_alive() {
    with_expression("list(saved, keyword=keyword_value)\n", |expression| {
        let context = Rc::new(Context::default());
        let original = call(expression);
        let prior = observations([(key(&original.arguments.args[0]), Some(Type::Bytes))]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let copy = original.clone();
        let addresses = call_keys(&copy);
        let guard = context
            .observe_call_expansion(original, &[&original.arguments.args[0]], &copy)
            .expect("active guard");
        let calls = Cell::new(0);
        let callback = |_: &Expr| {
            calls.set(calls.get() + 1);
            Some(Type::Int)
        };
        assert_eq!(
            context.infer_with_calls(&copy.arguments.args[0], &BTreeMap::new(), &callback),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 0);
        drop(guard);
        assert_current(&context, Some(&prior));
        assert_removed(&context, &addresses);
        assert_eq!(
            context.infer_with_calls(&copy.arguments.args[0], &BTreeMap::new(), &callback),
            Some(Type::Int),
            "the live copied AST no longer has its scoped saved record"
        );
        assert_eq!(calls.get(), 1);
        drop(copy);
        assert_current(&context, Some(&prior));
    });
}

#[test]
fn nested_expanded_call_drop_restores_outer_keys_and_owner() {
    with_expression("list(*(saved,))\n", |expression| {
        let context = Rc::new(Context::default());
        let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
        let signature = context
            .collection_constructor("list", PythonTarget::Py314)
            .expect("real builtin signature");
        let original = call(expression);
        let Expr::Starred(starred) = &original.arguments.args[0] else {
            panic!("starred fixture")
        };
        let Expr::Tuple(list) = starred.value.as_ref() else {
            panic!("list fixture")
        };
        let prior = observations([(key(&list.elts[0]), Some(Type::Bytes))]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let outer = scopes
            .collection_expanded_call(original, &signature)
            .expect("outer expansion");
        let outer_map = current(&context).expect("outer copied map");
        let outer_keys = call_keys(&outer);
        let inner = scopes
            .collection_expanded_call(original, &signature)
            .expect("inner expansion");
        let inner_keys = call_keys(&inner);
        assert!(
            outer_keys
                .iter()
                .all(|address| !inner_keys.contains(address)),
            "two live copied allocations must be distinct"
        );
        assert_eq!(
            context.infer_with_calls(&inner.arguments.args[0], &BTreeMap::new(), &|_| panic!(
                "known inner record"
            )),
            Some(Type::Bytes)
        );
        drop(inner);
        assert_current(&context, Some(&outer_map));
        assert_removed(&context, &inner_keys);
        assert_eq!(
            context.infer_with_calls(&outer.arguments.args[0], &BTreeMap::new(), &|_| panic!(
                "outer record must remain"
            )),
            Some(Type::Bytes)
        );
        drop(outer);
        assert_current(&context, Some(&prior));
        assert_removed(&context, &outer_keys);
    });
}

#[test]
fn expanded_call_restores_after_early_none_and_controlled_panic() {
    with_expression("list(*(unknown,))\n", |expression| {
        let context = Rc::new(Context::default());
        let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
        let signature = context
            .collection_constructor("list", PythonTarget::Py314)
            .expect("real builtin signature");
        let original = call(expression);
        let Expr::Starred(starred) = &original.arguments.args[0] else {
            panic!("starred fixture")
        };
        let Expr::Tuple(list) = starred.value.as_ref() else {
            panic!("list fixture")
        };
        let prior = observations([(key(&list.elts[0]), None)]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        let last_addresses = RefCell::new(Vec::new());
        let early = (|| {
            let expanded = scopes.collection_expanded_call(original, &signature)?;
            last_addresses.replace(call_keys(&expanded));
            context.infer_with_calls(&expanded.arguments.args[0], &BTreeMap::new(), &|_| {
                panic!("known None must not fall back")
            })
        })();
        assert_eq!(early, None);
        assert_current(&context, Some(&prior));
        assert_removed(&context, &last_addresses.borrow());
        let failure = catch_unwind(AssertUnwindSafe(|| {
            let expanded = scopes
                .collection_expanded_call(original, &signature)
                .expect("panic expansion");
            last_addresses.replace(call_keys(&expanded));
            assert_eq!(
                context.infer_with_calls(
                    &expanded.arguments.args[0],
                    &BTreeMap::new(),
                    &|_| panic!("known None")
                ),
                None
            );
            panic!("controlled copied-call panic");
        }));
        assert!(failure.is_err());
        assert_current(&context, Some(&prior));
        assert_removed(&context, &last_addresses.borrow());
        assert!(context.observed_expression_types.try_borrow_mut().is_ok());
    });
}

#[test]
fn inactive_expansion_and_refused_expansion_preserve_default_state() {
    with_expression("list(*(saved,))\n", |expression| {
        let context = Rc::new(Context::default());
        let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
        let signature = context
            .collection_constructor("list", PythonTarget::Py314)
            .expect("real builtin signature");
        let expanded = scopes
            .collection_expanded_call(call(expression), &signature)
            .expect("inactive literal expansion");
        assert_current(&context, None);
        let calls = Cell::new(0);
        assert_eq!(
            context.infer_with_calls(&expanded.arguments.args[0], &BTreeMap::new(), &|_| {
                calls.set(calls.get() + 1);
                Some(Type::Str)
            }),
            Some(Type::Str)
        );
        assert_eq!(calls.get(), 1);
        drop(expanded);
        assert_current(&context, None);
    });
    with_expression("list(*values)\n", |expression| {
        let context = Rc::new(Context::default());
        let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
        let signature = context
            .collection_constructor("list", PythonTarget::Py314)
            .expect("real builtin signature");
        let prior = observations([]);
        context
            .observed_expression_types
            .replace(Some(prior.clone()));
        assert!(scopes
            .collection_expanded_call(call(expression), &signature)
            .is_none());
        assert_current(&context, Some(&prior));
    });
}

#[test]
fn observation_maps_do_not_cross_contexts() {
    with_expression("value\n", |expression| {
        let first = Context::default();
        let second = Context::default();
        let first_map = observations([(key(expression), None)]);
        first
            .observed_expression_types
            .replace(Some(first_map.clone()));
        assert_eq!(
            first.infer_with_calls(expression, &BTreeMap::new(), &|_| panic!("known None")),
            None
        );
        let calls = Cell::new(0);
        assert_eq!(
            second.infer_with_calls(expression, &BTreeMap::new(), &|_| {
                calls.set(calls.get() + 1);
                Some(Type::Bytes)
            }),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 1);
        assert_current(&first, Some(&first_map));
        assert_current(&second, None);
    });
}

#[test]
fn starred_list_shape_is_not_expanded_with_any_observation_state() {
    with_expression("list(*[saved])\n", |expression| {
        let context = Rc::new(Context::default());
        let scopes = Scopes::new(&[], context.clone(), "guard_fixture".into());
        let signature = context
            .collection_constructor("list", PythonTarget::Py314)
            .expect("real builtin signature");
        let original = call(expression);
        let Expr::Starred(starred) = &original.arguments.args[0] else {
            panic!("starred fixture")
        };
        let Expr::List(list) = starred.value.as_ref() else {
            panic!("list fixture")
        };
        assert!(scopes
            .collection_expanded_call(original, &signature)
            .is_none());
        assert_current(&context, None);
        for checked in [Some(Some(Type::Bytes)), Some(None), None] {
            let entries = checked
                .into_iter()
                .flat_map(|actual| [(key(&starred.value), actual), (key(&list.elts[0]), actual)]);
            let prior = observations(entries);
            let before = prior.lock().expect("observed types").clone();
            context
                .observed_expression_types
                .replace(Some(prior.clone()));
            assert!(scopes
                .collection_expanded_call(original, &signature)
                .is_none());
            assert_current(&context, Some(&prior));
            assert_eq!(*prior.lock().expect("observed types"), before);
            assert!(context.observed_expression_types.try_borrow_mut().is_ok());
        }
    });
}
