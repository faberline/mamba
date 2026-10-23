//! Recursive declaration failures and reveal notes use original source spans.
use super::*;
use crate::infrastructure::mypy::types::{Literal, Node};
use quality_syntax::python_ast::{visitor, ExprCall};

impl Checker<'_> {
    pub(super) fn check_recursive_alias_declaration(
        &mut self,
        id: u32,
        statement: &Stmt,
        value: Option<&Expr>,
    ) {
        let Some(value) = value else { return };
        let alias = self.context.alias(id);
        if self.context.unguarded_alias_cycle(id) {
            if matches!(value, Expr::BinOp(_)) {
                self.emit(
                    "misc",
                    "Invalid recursive alias: a union item of itself".into(),
                    value,
                );
            } else {
                self.emit(
                    "misc",
                    format!(
                        "Cannot resolve name \"{}\" (possible cyclic definition)",
                        alias.name
                    ),
                    statement,
                );
                if let Expr::Name(name) = value {
                    if name.id.as_str() != alias.name
                        && value.start() < statement.end()
                        && statement.start() < value.start()
                    {
                        // The first declaration points to a later unresolved definition.
                        if self
                            .scopes
                            .first_definition(name.id.as_str())
                            .is_some_and(|offset| offset > value.start())
                        {
                            self.emit(
                                "misc",
                                format!(
                                    "Cannot resolve name \"{}\" (possible cyclic definition)",
                                    name.id
                                ),
                                value,
                            );
                        }
                    }
                }
            }
        }
        if alias.invalid_variable.is_some() && self.context.recursive_alias(id) {
            let mut references = ForwardStrings::default();
            references.visit_expr(value);
            for expression in references.values {
                let Expr::StringLiteral(string) = expression else {
                    continue;
                };
                let spelling = string.value.to_str();
                if spelling == alias.name || spelling.starts_with(&format!("{}[", alias.name)) {
                    let point = ruff_text_size::TextRange::at(
                        expression.start(),
                        ruff_text_size::TextSize::new(1),
                    );
                    self.emit_with_hint("valid-type", format!("Variable \"{}\" is not valid as a type", self.scopes.qualified_name(&alias.name)), &point, Some("See https://mypy.readthedocs.io/en/stable/common_issues.html#variables-vs-type-aliases".into()));
                }
            }
        }
    }

    pub(super) fn check_alias_reveal(&mut self, call: &ExprCall) -> bool {
        if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "reveal_type") {
            return false;
        }
        let [value] = call.arguments.args.as_ref() else {
            return false;
        };
        let Some(ty) = self
            .scopes
            .class_alias_reveal(value)
            .or_else(|| {
                if let Expr::BooleanLiteral(boolean) = value {
                    Some(
                        self.context
                            .intern(Node::LiteralValue(Literal::Bool(boolean.value))),
                    )
                } else {
                    None
                }
            })
            .or_else(|| {
                let Expr::NumberLiteral(number) = value else {
                    return None;
                };
                let mamba_language::application::quality_syntax::python_ast::Number::Int(integer) =
                    &number.value
                else {
                    return None;
                };
                Some(
                    self.context
                        .intern(Node::LiteralValue(Literal::Int(integer.as_i64()?))),
                )
            })
            .or_else(|| self.infer(value))
        else {
            return true;
        };
        let override_name = self
            .invalid_alias_reveal(value)
            .or_else(|| self.local_typed_dict_reveal(ty));
        if !self.loop_reveal(
            ty,
            override_name.clone(),
            value.range(),
            self.context.source_declaration_site(value),
        ) {
            self.emit_alias_reveal(ty, override_name, value.range());
        }
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn emit_alias_reveal(
        &mut self,
        ty: types::Type,
        override_name: Option<String>,
        location: ruff_text_size::TextRange,
    ) {
        let name =
            override_name.unwrap_or_else(|| self.context.revealed_name_for(ty, self.python_target));
        let mut diagnostic = super::super::super::diagnostics::at(
            self.source,
            QualityTool::Mypy,
            "misc",
            format!("Revealed type is \"{name}\""),
            location,
        );
        diagnostic.severity = "note".into();
        if !self.directives.suppresses(diagnostic.location.line, "misc") {
            if let Some(message) = self.directives.hint(diagnostic.location.line, "misc") {
                let mut uncovered = super::super::super::diagnostics::at(
                    self.source,
                    QualityTool::Mypy,
                    "",
                    message,
                    location,
                );
                uncovered.severity = "note".into();
                self.diagnostics.push(diagnostic);
                self.diagnostics.push(uncovered);
                return;
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn invalid_alias_reveal(&self, value: &Expr) -> Option<String> {
        if let Expr::Name(name) = value {
            let offset = self.scopes.first_definition(name.id.as_str())?;
            return self.invalid_alias_annotations.get(&offset).cloned();
        }
        let Expr::Call(call) = value else { return None };
        self.scopes
            .typing_cast_signature(&call.func, &self.scopes.environment())?;
        let Expr::Name(name) = call.arguments.args.first()? else {
            return None;
        };
        (self.scopes.annotation(&call.arguments.args[0]).is_none()
            && self.scopes.has_binding(name.id.as_str()))
        .then(|| format!("{}?", name.id))
    }

    fn local_typed_dict_reveal(&self, ty: types::Type) -> Option<String> {
        let (class, definition) = self.context.typed_dict(ty)?;
        let existing = self.context.revealed_name_for(ty, self.python_target);
        if existing.starts_with("TypedDict(") {
            return None;
        }
        let module = class
            .namespace
            .split_once(':')
            .map_or(class.namespace.as_str(), |(_, module)| module);
        let fields = definition
            .items
            .iter()
            .map(|field| {
                let optional = if field.required { "" } else { "?" };
                let readonly = if field.readonly { "=" } else { "" };
                format!(
                    "'{}'{optional}{readonly}: {}",
                    field.name,
                    self.context.revealed_name_for(field.ty, self.python_target)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("TypedDict({module}.{}, {{{fields}}})", class.name))
    }

    pub(super) fn remember_invalid_alias_annotation(
        &mut self,
        assignment: &quality_syntax::python_ast::StmtAnnAssign,
    ) {
        let Expr::Subscript(subscript) = assignment.annotation.as_ref() else {
            return;
        };
        let variables = self.scopes.variables();
        let Some(id) = self
            .context
            .annotation_binding(&subscript.value, &variables)
            .and_then(|ty| self.context.alias_id(ty))
        else {
            return;
        };
        let alias = self.context.alias(id);
        if alias.invalid_variable.is_none() {
            return;
        }
        let arguments = match subscript.slice.as_ref() {
            Expr::Tuple(tuple) => tuple.elts.iter().collect::<Vec<_>>(),
            expression => vec![expression],
        };
        let names = arguments
            .into_iter()
            .map(|argument| {
                self.scopes
                    .annotation(argument)
                    .map_or_else(|| "Any".into(), |ty| self.context.name(ty))
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.invalid_alias_annotations.insert(
            assignment.target.start(),
            format!("{}?[{names}]", alias.name),
        );
    }
}

#[derive(Default)]
struct ForwardStrings<'a> {
    values: Vec<&'a Expr>,
}

impl<'a> visitor::Visitor<'a> for ForwardStrings<'a> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::StringLiteral(_)) {
            self.values.push(expression);
        } else {
            visitor::walk_expr(self, expression);
        }
    }
}
