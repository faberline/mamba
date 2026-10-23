//! Real parser/type controls for checked mappings. All Rust execution is Root-owned.
use super::*;
use crate::infrastructure::mypy::{classes, types::Container};
use mamba_language::application::quality_syntax::{self, python_ast::Stmt, PythonTarget};
use std::cell::Cell;

fn with_call(source: &str, test: impl FnOnce(&ExprCall)) {
    let document = quality_syntax::parse(source, PythonTarget::Py314, false).unwrap();
    assert!(document.is_valid(), "{:?}", document.issues);
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("one expression statement");
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        panic!("one original call");
    };
    test(call);
}

fn replacements<'a, 's>(call: &'a ExprCall, template: &'s str) -> Vec<CheckedReplacement<'a, 's>> {
    match map_checked_fields(call, parse_checked_fields(template).unwrap()).unwrap() {
        FormatPlan::Replacements(values) => values,
        FormatPlan::MixedNumbering => panic!("expected replacements"),
    }
}

fn field_class(context: &types::Context, offset: u32, name: &str, ty: Option<Type>) -> Type {
    let id = context.declare_class("mapping_control", "Box", offset);
    let mut class = context.class(id).as_ref().clone();
    class.fields.insert(
        name.into(),
        classes::Field {
            ty,
            needs_annotation: false,
            class_access: false,
            readonly: false,
            final_attribute: false,
            class_var: false,
            abstract_field: false,
        },
    );
    let instance = class.instance;
    context.replace_class(id, class);
    instance
}

#[test]
fn attribute_uses_original_checked_receiver_and_declared_member() {
    let context = types::Context::default();
    let instance = field_class(&context, 1, "value", Some(Type::Bytes));
    with_call(r#""{0.value}".format(Box())"#, |call| {
        let values = replacements(call, "{0.value}");
        assert!(std::ptr::eq(values[0].expression, &call.arguments.args[0]));
        let count = Cell::new(0);
        let ty = checked_replacement_type(&context, &values[0], &|original| {
            count.set(count.get() + 1);
            assert!(std::ptr::eq(original, &call.arguments.args[0]));
            Some(Some(instance))
        });
        assert_eq!(ty, Some(Type::Bytes));
        assert_eq!(count.get(), 1);
        assert!(parse_fields("{0.value}").is_none());
    });
}

#[test]
fn double_star_projects_checked_mapping_value_once() {
    let context = types::Context::default();
    let mapping = context.intern(Node::Container(
        Container::Dict,
        vec![Type::Str, Type::Bytes],
    ));
    with_call(r#""{value}".format(**{"value": raw})"#, |call| {
        let values = replacements(call, "{value}");
        let original = &call.arguments.keywords[0].value;
        assert!(std::ptr::eq(values[0].expression, original));
        assert!(matches!(values[0].projection, Projection::Mapping));
        let count = Cell::new(0);
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|expression| {
                count.set(count.get() + 1);
                assert!(std::ptr::eq(expression, original));
                Some(Some(mapping))
            }),
            Some(Type::Bytes)
        );
        assert_eq!(count.get(), 1);
    });
}

#[test]
fn first_double_star_and_explicit_keyword_priority_keep_original_addresses() {
    with_call(r#""{value}".format(**first, **second)"#, |call| {
        let values = replacements(call, "{value}");
        assert!(std::ptr::eq(
            values[0].expression,
            &call.arguments.keywords[0].value
        ));
        assert!(matches!(values[0].projection, Projection::Mapping));
    });
    with_call(r#""{value}".format(value=direct, **fallback)"#, |call| {
        let values = replacements(call, "{value}");
        assert!(std::ptr::eq(
            values[0].expression,
            &call.arguments.keywords[0].value
        ));
        assert!(matches!(values[0].projection, Projection::Direct));
    });
}

#[test]
fn fixed_tuple_star_keeps_any_and_variadic_tuple_star_keeps_item_type() {
    let context = types::Context::default();
    for (ty, expected) in [
        (context.intern(Node::Tuple(vec![Type::Bytes])), Type::Any),
        (
            context.intern(Node::VariadicTuple(Type::Bytes)),
            Type::Bytes,
        ),
        (context.intern(Node::VariadicTuple(Type::Str)), Type::Str),
    ] {
        with_call(r#""{0}".format(*items)"#, |call| {
            let values = replacements(call, "{0}");
            let Expr::Starred(star) = &call.arguments.args[0] else {
                panic!("starred node");
            };
            assert!(std::ptr::eq(values[0].expression, star.value.as_ref()));
            assert!(matches!(values[0].projection, Projection::Sequence));
            let count = Cell::new(0);
            assert_eq!(
                checked_replacement_type(&context, &values[0], &|original| {
                    count.set(count.get() + 1);
                    assert!(std::ptr::eq(original, star.value.as_ref()));
                    Some(Some(ty))
                }),
                Some(expected)
            );
            assert_eq!(count.get(), 1);
        });
    }
}

#[test]
fn sequence_star_uses_checked_sequence_item_without_argument_inference() {
    let context = types::Context::default();
    let ty = context.intern(Node::Container(Container::Sequence, vec![Type::Bytes]));
    with_call(r#""{0}".format(*items)"#, |call| {
        let values = replacements(call, "{0}");
        let calls = Cell::new(0);
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|_| {
                calls.set(calls.get() + 1);
                Some(Some(ty))
            }),
            Some(Type::Bytes)
        );
        assert_eq!(calls.get(), 1);
    });
}

#[test]
fn explicit_position_precedes_first_star_and_stars_keep_field_order() {
    with_call(r#""{0}".format(*items, direct)"#, |call| {
        let values = replacements(call, "{0}");
        assert!(std::ptr::eq(values[0].expression, &call.arguments.args[1]));
        assert!(matches!(values[0].projection, Projection::Direct));
    });
    with_call(r#""{1} {0}".format(*first, *second)"#, |call| {
        let values = replacements(call, "{1} {0}");
        let Expr::Starred(first) = &call.arguments.args[0] else {
            panic!("starred node");
        };
        assert_eq!(values.len(), 2);
        assert!(values
            .iter()
            .all(|value| std::ptr::eq(value.expression, first.value.as_ref())));
    });
}

#[test]
fn mixed_numbering_precedes_mapping_and_explicit_conversions() {
    for (source, template) in [
        (r#""{} {0}".format(raw)"#, "{} {0}"),
        (r#""{!r} {0!r}".format()"#, "{!r} {0!r}"),
        (r#""{.value} {0.value}".format()"#, "{.value} {0.value}"),
    ] {
        with_call(source, |call| {
            assert!(matches!(
                map_checked_fields(call, parse_checked_fields(template).unwrap()),
                Some(FormatPlan::MixedNumbering)
            ))
        });
    }
}

#[test]
fn observed_none_and_missing_record_stop_after_one_lookup() {
    let context = types::Context::default();
    for checked in [None, Some(None)] {
        with_call(r#""{value}".format(**mapping)"#, |call| {
            let values = replacements(call, "{value}");
            let count = Cell::new(0);
            assert_eq!(
                checked_replacement_type(&context, &values[0], &|_| {
                    count.set(count.get() + 1);
                    checked
                }),
                None
            );
            assert_eq!(count.get(), 1);
        });
    }
}

#[test]
fn checked_type_is_not_replaced_by_a_later_name_type() {
    let context = types::Context::default();
    let before = context.intern(Node::Container(
        Container::Mapping,
        vec![Type::Str, Type::Bytes],
    ));
    let later = context.intern(Node::Container(
        Container::Mapping,
        vec![Type::Str, Type::Str],
    ));
    assert_ne!(before, later);
    with_call(r#""{value}".format(**mapping)"#, |call| {
        let values = replacements(call, "{value}");
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|_| Some(Some(before))),
            Some(Type::Bytes)
        );
    });
}

#[test]
fn chained_declared_attributes_use_one_original_receiver_lookup() {
    let context = types::Context::default();
    let child = field_class(&context, 1, "value", Some(Type::Bytes));
    let outer = field_class(&context, 2, "child", Some(child));
    with_call(r#""{0.child.value}".format(obj)"#, |call| {
        let values = replacements(call, "{0.child.value}");
        let count = Cell::new(0);
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|_| {
                count.set(count.get() + 1);
                Some(Some(outer))
            }),
            Some(Type::Bytes)
        );
        assert_eq!(count.get(), 1);
    });
}

#[test]
fn any_and_untyped_attributes_keep_their_original_unknown_boundary() {
    let context = types::Context::default();
    let unknown = field_class(&context, 1, "value", None);
    with_call(r#""{0.value}".format(obj)"#, |call| {
        let values = replacements(call, "{0.value}");
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|_| Some(Some(Type::Any))),
            Some(Type::Any)
        );
        assert_eq!(
            checked_replacement_type(&context, &values[0], &|_| Some(Some(unknown))),
            None
        );
    });
}

#[test]
fn extra_explicit_arguments_remain_unsupported() {
    for (source, template) in [
        (r#""{0.value}".format(obj, extra)"#, "{0.value}"),
        (r#""{name}".format(other=extra, **values)"#, "{name}"),
    ] {
        with_call(source, |call| {
            assert!(map_checked_fields(call, parse_checked_fields(template).unwrap()).is_none())
        });
    }
}

#[test]
fn unsupported_accessors_and_specs_keep_complete_parse_rejection() {
    for value in [
        "{0[0]}",
        "{name..value}",
        "{name.}",
        "{name.é}",
        "{name.value:{width}}",
        "{name.value:date}",
        "{} {",
        "{0.value!x}",
    ] {
        assert!(parse_checked_fields(value).is_none(), "{value}");
    }
}
