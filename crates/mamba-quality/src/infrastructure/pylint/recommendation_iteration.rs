//! Iteration recommendations preserve original lookup, write, and scope exclusions.
use super::{callables::Target, display, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Comprehension, Expr, ExprSubscript, Number, Stmt, StmtFor,
};
use ruff_text_size::Ranged;

#[derive(Default)]
struct Subscripts<'a> {
    values: Vec<&'a ExprSubscript>,
    same_scope: bool,
    filter_bindings: bool,
    shadowed: std::collections::BTreeSet<String>,
}
impl<'a> Visitor<'a> for Subscripts<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if self.same_scope && matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            return;
        }
        if let Stmt::FunctionDef(function) = statement {
            let mut declarations =
                super::super::project::symbols::Symbols::collect(&function.body, "", false);
            declarations.parameters(&function.parameters);
            let saved = self.shadowed.clone();
            self.shadowed.extend(declarations.bindings.keys().cloned());
            self.visit_body(&function.body);
            self.shadowed = saved;
            return;
        }
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if self.same_scope
            && matches!(
                expression,
                Expr::Lambda(_)
                    | Expr::ListComp(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
                    | Expr::Generator(_)
            )
        {
            return;
        }
        if let Expr::Subscript(value) = expression {
            if !self.filter_bindings
                || !matches!(value.slice.as_ref(), Expr::Name(name) if self.shadowed.contains(name.id.as_str()))
            {
                self.values.push(value);
            }
        }
        walk_expr(self, expression);
    }
}
fn starred_set(expression: &Expr) -> bool {
    match expression {
        Expr::Starred(_) => true,
        Expr::Set(value) => value.elts.iter().any(starred_set),
        _ => false,
    }
}
fn same_object(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
        (Expr::Attribute(left), Expr::Attribute(right)) => {
            left.attr.as_str() == right.attr.as_str()
        }
        _ => false,
    }
}
impl Checker<'_> {
    fn recommendation_builtin(&self, expression: &Expr, name: &str) -> bool {
        if self.stdlib_name(expression).as_deref() == Some(format!("builtins.{name}").as_str()) {
            return true;
        }
        self.expression_target_in(expression, Context::Scope(self.scopes.current_index()))
            .is_some_and(|resolved| match resolved.target {
                Target::Class(class) => class.identity.0 == "builtins" && class.name == name,
                Target::Function(signature) => signature.name == name
                    && matches!(resolved.context, Context::Module(ref module) if module == "builtins"),
                _ => false,
            })
            || matches!(expression, Expr::Name(value) if value.id.as_str() == name && !self.scopes.declared(name))
    }
    fn recommend_enumerate(&mut self, value: &StmtFor) {
        let (Expr::Call(range), Expr::Name(target)) = (value.iter.as_ref(), value.target.as_ref())
        else {
            return;
        };
        if !self.recommendation_builtin(&range.func, "range")
            || !(1..=2).contains(&range.arguments.args.len())
        {
            return;
        }
        if range.arguments.args.len() == 2
            && !matches!(&range.arguments.args[0], Expr::NumberLiteral(value) if match &value.value { Number::Int(value) => value.to_string() == "0", Number::Float(value) => *value == 0.0, Number::Complex { real, imag } => *real == 0.0 && *imag == 0.0 })
            && !matches!(&range.arguments.args[0], Expr::BooleanLiteral(value) if !value.value)
        {
            return;
        }
        let Some(Expr::Call(length)) = range.arguments.args.last() else {
            return;
        };
        if !self.recommendation_builtin(&length.func, "len") || length.arguments.args.len() != 1 {
            return;
        }
        let object = &length.arguments.args[0];
        if matches!(object, Expr::Name(name) if name.id.as_str() == "self")
            && self.method_context.as_deref() == Some("__iter__")
        {
            return;
        }
        let mut subscripts = Subscripts {
            same_scope: true,
            ..Subscripts::default()
        };
        subscripts.visit_body(&value.body);
        if subscripts.values.iter().any(|value| {
            matches!(value.slice.as_ref(), Expr::Name(name) if name.id == target.id)
                && same_object(object, &value.value)
        }) {
            self.emit(
                "C0200",
                "consider-using-enumerate",
                "Consider using enumerate instead of iterating with range and len",
                value.range(),
            );
        }
    }
    fn iterating_dictionary(&self, expression: &Expr) -> Option<String> {
        let object = match expression {
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Attribute(value) if value.attr.as_str() == "keys" => {
                    if matches!(
                        self.operand(&value.value),
                        super::operand_inference::Operand::Instance(_, _)
                    ) && matches!(self.callable_target(&call.func), Some(Target::Function(_)))
                    {
                        return Some(display::expression(&value.value, &self.source.text));
                    }
                    value.value.as_ref()
                }
                _ => return None,
            },
            Expr::Name(_) | Expr::Attribute(_) => expression,
            _ => return None,
        };
        self.unpacked_value(object, Context::Scope(self.scopes.current_index()))
            .is_some_and(|value| value.dictionary && value.view.is_none())
            .then(|| display::expression(object, &self.source.text))
    }
    fn matching_dictionary_subscript(
        &self,
        value: &ExprSubscript,
        key: &str,
        object: &str,
    ) -> bool {
        matches!(value.value.as_ref(), Expr::Name(_) | Expr::Attribute(_))
            && matches!(value.slice.as_ref(), Expr::Name(name) if name.id.as_str() == key)
            && display::expression(&value.value, &self.source.text) == object
    }
    pub(super) fn check_iteration_recommendations(&mut self, value: &StmtFor) {
        self.recommend_enumerate(value);
        if let (Some(object), Expr::Name(target)) = (
            self.iterating_dictionary(&value.iter),
            value.target.as_ref(),
        ) {
            let mut subscripts = Subscripts {
                filter_bindings: true,
                ..Subscripts::default()
            };
            subscripts.visit_body(&value.body);
            for subscript in subscripts.values {
                if !self.matching_dictionary_subscript(subscript, target.id.as_str(), &object) {
                    continue;
                }
                if self
                    .scopes
                    .last_assignment(target.id.as_str())
                    .is_some_and(|range| {
                        let index =
                            ruff_source_file::LineIndex::from_source_text(&self.source.text);
                        index.line_column(range.start(), &self.source.text).line
                            > index.line_column(value.start(), &self.source.text).line
                            && range.start() < subscript.start()
                    })
                {
                    continue;
                }
                if !subscript.ctx.is_load() {
                    break;
                }
                self.emit(
                    "C0206",
                    "consider-using-dict-items",
                    "Consider iterating with .items()",
                    value.range(),
                );
                break;
            }
        }
        self.recommend_sequence(&value.iter);
    }
    fn recommend_sequence(&mut self, expression: &Expr) {
        if matches!(expression, Expr::Set(_)) && !starred_set(expression) {
            self.emit(
                "C0208",
                "use-sequence-for-iteration",
                "Use a sequence type when iterating over values",
                expression.range(),
            );
        }
    }
    pub(super) fn check_iteration_comprehensions(
        &mut self,
        generators: &[Comprehension],
        values: &[&Expr],
    ) {
        let mut subscripts = Subscripts::default();
        for value in values {
            subscripts.visit_expr(value);
        }
        for generator in generators {
            subscripts.visit_expr(&generator.iter);
            for condition in &generator.ifs {
                subscripts.visit_expr(condition);
            }
        }
        for generator in generators {
            if let (Some(object), Expr::Name(target)) = (
                self.iterating_dictionary(&generator.iter),
                &generator.target,
            ) {
                if subscripts.values.iter().any(|value| {
                    self.matching_dictionary_subscript(value, target.id.as_str(), &object)
                }) {
                    let index = ruff_source_file::LineIndex::from_source_text(&self.source.text);
                    let line = index.line_column(generator.start(), &self.source.text).line;
                    self.emit(
                        "C0206",
                        "consider-using-dict-items",
                        "Consider iterating with .items()",
                        super::super::diagnostics::point(
                            index.line_start(line, &self.source.text).to_usize(),
                        ),
                    );
                }
            }
            self.recommend_sequence(&generator.iter);
        }
    }
}
