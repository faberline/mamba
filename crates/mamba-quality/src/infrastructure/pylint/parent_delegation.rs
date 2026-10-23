//! Trivial super forwarding retains the original defaults and annotation gates.
use super::{
    callables::{Signature, Target},
    docstring, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, Parameters, Stmt, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

fn equivalent_call(parameters: &Parameters, call: &ExprCall) -> bool {
    let expected: Vec<_> = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .filter(|value| value.parameter.name.as_str() != "self")
        .map(|value| Some(value.parameter.name.as_str()))
        .collect();
    let mut positional = Vec::new();
    let mut starred = BTreeSet::new();
    for argument in &call.arguments.args {
        match argument {
            Expr::Starred(value) if matches!(value.value.as_ref(), Expr::Name(_)) => {
                let Expr::Name(value) = value.value.as_ref() else {
                    unreachable!()
                };
                starred.insert(value.id.as_str());
            }
            Expr::Name(value) => positional.push(Some(value.id.as_str())),
            _ => positional.push(None),
        }
    }
    let mut keywords = BTreeSet::new();
    let mut star_keywords = BTreeSet::new();
    for keyword in &call.arguments.keywords {
        if let Some(name) = &keyword.arg {
            keywords.insert(Some(name.as_str()));
        } else if let Expr::Name(value) = &keyword.value {
            star_keywords.insert(value.id.as_str());
        } else {
            keywords.insert(None);
        }
    }
    if parameters
        .vararg
        .as_ref()
        .map_or(!starred.is_empty(), |value| {
            !starred.contains(value.name.as_str())
        })
        || parameters
            .kwarg
            .as_ref()
            .map_or(!star_keywords.is_empty(), |value| {
                !star_keywords.contains(value.name.as_str())
            })
        || expected != positional
    {
        return false;
    }
    let only: BTreeSet<_> = parameters
        .kwonlyargs
        .iter()
        .map(|value| Some(value.parameter.name.as_str()))
        .collect();
    only.iter().all(|name| keywords.contains(name))
        && keywords
            .iter()
            .all(|name| positional.contains(name) || only.contains(name))
}

fn same_default(left: &Expr, right: &Expr, left_source: &str, right_source: &str) -> bool {
    if matches!(left, Expr::NoneLiteral(_)) && matches!(right, Expr::NoneLiteral(_)) {
        return true;
    }
    match (left, right) {
        (Expr::StringLiteral(left), Expr::StringLiteral(right)) => {
            left.value.to_str() == right.value.to_str()
        }
        (Expr::BytesLiteral(left), Expr::BytesLiteral(right)) => {
            left.value.bytes().eq(right.value.bytes())
        }
        (Expr::NumberLiteral(left), Expr::NumberLiteral(right)) => left.value == right.value,
        (Expr::BooleanLiteral(left), Expr::BooleanLiteral(right)) => left.value == right.value,
        (Expr::Name(_), Expr::Name(_)) => {
            super::display::expression(left, left_source)
                == super::display::expression(right, right_source)
        }
        _ => false,
    }
}

fn same_defaults(
    parent: &Parameters,
    current: &Parameters,
    parent_source: &str,
    current_source: &str,
) -> bool {
    let current: BTreeMap<_, _> = current
        .posonlyargs
        .iter()
        .chain(&current.args)
        .chain(&current.kwonlyargs)
        .map(|value| (value.parameter.name.as_str(), value.default.as_deref()))
        .collect();
    parent
        .posonlyargs
        .iter()
        .chain(&parent.args)
        .chain(&parent.kwonlyargs)
        .all(|parameter| {
            let before = parameter.default.as_deref();
            let after = current
                .get(parameter.parameter.name.as_str())
                .copied()
                .flatten();
            match (before, after) {
                (None, None) => true,
                (Some(before), Some(after)) => {
                    same_default(before, after, parent_source, current_source)
                }
                _ => false,
            }
        })
}

fn annotations(parameters: &Parameters, source: &str) -> Vec<String> {
    parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .filter_map(|value| value.parameter.annotation.as_deref())
        .map(|value| super::display::expression(value, source))
        .collect()
}

impl Checker<'_> {
    pub(super) fn inherited_method_docstring(&self, function: &StmtFunctionDef) -> bool {
        if !self.scopes.in_class() {
            return false;
        }
        let Some((class, context)) = self.class_stack.last().cloned() else {
            return false;
        };
        self.lineage(class, context)
            .into_iter()
            .skip(1)
            .any(|(parent, _)| {
                !(parent.identity.0 == "builtins" && parent.name == "object")
                    && matches!(
                        parent.first_members.get(function.name.as_str()),
                        Some(Target::Function(_))
                    )
            })
    }
    pub(super) fn check_parent_delegation(&mut self, function: &StmtFunctionDef) {
        if !function.decorator_list.is_empty() {
            return;
        }
        let body = &function.body[usize::from(docstring(&function.body))..];
        let [statement] = body else { return };
        let expression = match statement {
            Stmt::Expr(value) => value.value.as_ref(),
            Stmt::Return(value) => {
                let Some(value) = value.value.as_deref() else {
                    return;
                };
                value
            }
            _ => return,
        };
        let Expr::Call(call) = expression else { return };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        if attribute.attr.as_str() != function.name.as_str() {
            return;
        }
        let Some(value) = self.expression_target(&attribute.value) else {
            return;
        };
        let Target::Super(proxy) = value.target else {
            return;
        };
        let Some((class, context)) = self.class_stack.last().cloned() else {
            return;
        };
        if proxy.pointer != class.identity || !proxy.instance || proxy.class.name != class.name {
            return;
        }
        if function.name.as_str() == "__hash__" && class.methods.contains_key("__eq__") {
            return;
        }
        let Ok(order) = self.mro(class, context) else {
            return;
        };
        for (parent, _) in order.into_iter().skip(1) {
            if !parent.first_members.contains_key(function.name.as_str()) {
                continue;
            }
            let Some(Target::Function(signature)) =
                parent.first_members.get(function.name.as_str())
            else {
                return;
            };
            let Some(parameters) = &signature.parameter_ast else {
                return;
            };
            let source = self
                .modules
                .source(self.source.project, &parent.identity.0)
                .unwrap_or(&self.source.text);
            if !same_defaults(parameters, &function.parameters, source, &self.source.text) {
                return;
            }
            if signature.variadic
                && (function.parameters.vararg.is_none()
                    || function.parameters.args.len() > parameters.args.len())
            {
                return;
            }
            let before = annotations(parameters, source);
            let after = annotations(&function.parameters, &self.source.text);
            if !before.is_empty() && !after.is_empty() && before != after {
                return;
            }
            if let (Some(before), Some(after)) = (&signature.return_ast, &function.returns) {
                if super::display::expression(before, source)
                    != super::display::expression(after, &self.source.text)
                {
                    return;
                }
            }
            break;
        }
        if equivalent_call(&function.parameters, call) {
            self.emit(
                "W0246",
                "useless-parent-delegation",
                &format!(
                    "Useless parent or super() delegation in method '{}'",
                    function.name
                ),
                self.declaration_range(function.name.range()),
            );
        }
    }

    pub(super) fn inherited_unused_argument(&self, function: &StmtFunctionDef, name: &str) -> bool {
        if !self.scopes.parent_is_class() {
            return false;
        }
        if super::defaults::fixed().special_method(function.name.as_str())
            && !matches!(function.name.as_str(), "__init__" | "__new__")
        {
            return true;
        }
        let Some((class, context)) = self.class_stack.last().cloned() else {
            return false;
        };
        for (parent, _) in self.lineage(class, context).into_iter().skip(1) {
            let Some(method) = parent.methods.get(function.name.as_str()) else {
                continue;
            };
            return signature_has_name(&method.signature, name);
        }
        false
    }
}

fn signature_has_name(signature: &Signature, name: &str) -> bool {
    signature
        .parameters
        .iter()
        .any(|parameter| parameter.name == name)
        || signature.parameter_ast.as_ref().is_some_and(|parameters| {
            parameters
                .vararg
                .as_ref()
                .is_some_and(|value| value.name.as_str() == name)
                || parameters
                    .kwarg
                    .as_ref()
                    .is_some_and(|value| value.name.as_str() == name)
        })
}
