//! Unpacking checks retain inferred lengths, original messages, and source details.
use super::{resolution::Context, unpacking_values::Unpacked, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, Number, StmtAssign, StmtFor};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    fn unpacking_details(&self, value: &Unpacked, range: TextRange) -> String {
        let Some(definition) = value.definition else {
            return String::new();
        };
        let module = match &value.origin {
            Context::Module(module) => module.as_str(),
            Context::Scope(_) => self.module_name,
        };
        let source = self
            .modules
            .source(self.source.project, module)
            .unwrap_or(&self.source.text);
        let inferred_line = LineIndex::from_source_text(source)
            .line_column(definition.start(), source)
            .line
            .get();
        let current_line = LineIndex::from_source_text(&self.source.text)
            .line_column(range.start(), &self.source.text)
            .line
            .get();
        if module == self.module_name && inferred_line == current_line {
            format!("'{}'", value.display)
        } else if module == self.module_name {
            format!("defined at line {inferred_line}")
        } else {
            format!("defined at line {inferred_line} of {module}")
        }
    }
    fn report_unbalanced(
        &mut self,
        dictionary: bool,
        labels: usize,
        values: i64,
        details: &str,
        range: TextRange,
    ) {
        let (rule, symbol, prefix) = if dictionary {
            ("W0644", "unbalanced-dict-unpacking", "dict")
        } else {
            ("W0632", "unbalanced-tuple-unpacking", "tuple")
        };
        self.emit(rule, symbol, &format!("Possible unbalanced {prefix} unpacking with {}{details}: left side has {labels} label{}, right side has {values} value{}", if dictionary { "" } else { "sequence " }, if labels == 1 { "" } else { "s" }, if values == 1 { "" } else { "s" }), range);
    }
    pub(super) fn check_unpacking(&mut self, assignment: &StmtAssign) {
        let targets = match assignment.targets.first() {
            Some(Expr::Tuple(value)) => &value.elts,
            Some(Expr::List(value)) => &value.elts,
            _ => return,
        };
        if targets
            .iter()
            .any(|value| matches!(value, Expr::Starred(_)))
        {
            return;
        }
        if self.class_stack.last().is_some_and(|(class, origin)| {
            !self
                .abstract_methods(class.clone(), origin.clone(), true)
                .is_empty()
        }) {
            return;
        }
        let Some(value) = self.unpacked_value(
            &assignment.value,
            Context::Scope(self.scopes.current_index()),
        ) else {
            return;
        };
        if matches!(assignment.value.as_ref(), Expr::Name(name) if value.vararg.as_deref() == Some(name.id.as_str()))
        {
            return;
        }
        if let Some(elements) = &value.elements {
            if elements.len() == targets.len() {
                return;
            }
            let details = if value.dictionary {
                super::display::expression(&assignment.value, &self.source.text)
            } else {
                self.unpacking_details(&value, assignment.range())
            };
            self.report_unbalanced(
                value.dictionary,
                targets.len(),
                elements.len() as i64,
                &details,
                assignment.range(),
            );
        } else if !value.iterable {
            let details = self.unpacking_details(&value, assignment.range());
            self.emit(
                "E0633",
                "unpacking-non-sequence",
                &format!(
                    "Attempting to unpack a non-sequence{}",
                    if details.is_empty() {
                        details
                    } else {
                        format!(" {details}")
                    }
                ),
                assignment.range(),
            );
        }
    }
    fn unpacked_element_length(&self, value: &Expr) -> i64 {
        match value {
            Expr::Tuple(value) => value.elts.len() as i64,
            Expr::List(value) => value.elts.len() as i64,
            Expr::Set(value) => value.elts.len() as i64,
            Expr::Dict(value) => value.items.len() as i64,
            Expr::StringLiteral(value) => value.value.to_str().chars().count() as i64,
            Expr::BytesLiteral(value) => value.value.bytes().count() as i64,
            Expr::Subscript(value) => {
                let Expr::Slice(slice) = value.slice.as_ref() else {
                    return 1;
                };
                let number = |value: Option<&Expr>| -> Option<i64> {
                    if let Expr::NumberLiteral(value) = value? {
                        if let Number::Int(value) = &value.value {
                            return value.to_string().parse().ok();
                        }
                    }
                    None
                };
                let Some(lower) = number(slice.lower.as_deref()) else {
                    return 1;
                };
                let Some(upper) = number(slice.upper.as_deref()) else {
                    return 1;
                };
                let step = if slice.step.is_none() {
                    1
                } else {
                    let Some(step) = number(slice.step.as_deref()) else {
                        return 1;
                    };
                    step
                };
                if step == 0 {
                    1
                } else {
                    ((upper as f64 - lower as f64) / step as f64).ceil() as i64
                }
            }
            _ => 1,
        }
    }
    pub(super) fn check_dictionary_unpacking(&mut self, statement: &StmtFor) {
        let Expr::Tuple(target) = statement.target.as_ref() else {
            return;
        };
        let Some(value) = self
            .unpacked_value(&statement.iter, Context::Scope(self.scopes.current_index()))
            .filter(|value| value.dictionary && value.view.is_some())
        else {
            return;
        };
        let Some(elements) = &value.elements else {
            return;
        };
        if elements.is_empty() {
            return;
        }
        let starred = target
            .elts
            .iter()
            .any(|value| matches!(value, Expr::Starred(_)));
        if value.view == Some("items") && (target.elts.len() == 2 || starred) {
            return;
        }
        for element in elements {
            let length = if value.view == Some("items") {
                2
            } else {
                self.unpacked_element_length(element)
            };
            if target.elts.len() as i64 != length
                && !(starred && length >= target.elts.len() as i64)
            {
                let details = super::display::expression(&statement.iter, &self.source.text);
                self.report_unbalanced(
                    true,
                    target.elts.len(),
                    length,
                    &details,
                    statement.range(),
                );
                break;
            }
        }
    }
}
