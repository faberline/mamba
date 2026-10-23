//! Standard dataclass fields form an ordered, typed constructor without execution.
use super::super::{
    signatures::{Kind, Parameter},
    types::Variables,
};
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    Arguments, Expr, ExprCall, Stmt, StmtClassDef,
};
use ruff_text_size::{Ranged, TextRange};
mod constructor;
mod conversions;
mod copy_replace;
mod field_data;
mod inheritance;
mod metadata;
mod ordering;
mod replace;
pub(super) mod transforms;
pub(in crate::infrastructure::mypy) use ordering::order_symbol;

#[derive(Clone)]
pub(in crate::infrastructure::mypy) struct Attribute {
    pub name: String,
    pub alias: Option<String>,
    pub ty: Type,
    pub required: bool,
    pub in_init: bool,
    pub keyword_only: bool,
    pub init_var: bool,
    pub neutral_frozen: bool,
    pub range: TextRange,
}

#[derive(Clone)]
pub(in crate::infrastructure::mypy) struct Definition {
    pub attributes: Vec<Attribute>,
    pub transformed: bool,
    pub field_specifiers: Vec<String>,
    pub class_options: bool,
    pub neutral_frozen: bool,
    pub frozen: bool,
    pub invalid_descriptors: Vec<(String, TextRange)>,
    pub invalid_order: Vec<TextRange>,
    pub duplicate_kw_only: Vec<TextRange>,
    pub slots_conflict: bool,
    pub sentinels: Vec<String>,
    pub generated_slots: bool,
    pub order: bool,
    pub invalid_eq: Option<TextRange>,
    pub order_conflicts: Vec<(String, TextRange)>,
    pub generated_replace: bool,
}

fn flag(arguments: Option<&Arguments>, name: &str, default: bool) -> bool {
    arguments
        .and_then(|arguments| {
            arguments.keywords.iter().find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|argument| argument.as_str() == name)
            })
        })
        .and_then(|keyword| match &keyword.value {
            Expr::BooleanLiteral(value) => Some(value.value),
            _ => None,
        })
        .unwrap_or(default)
}

impl Context {
    pub(super) fn populate_dataclass(
        &self,
        class: &mut Class,
        statement: &StmtClassDef,
        variables: &Variables,
    ) {
        let inherited = self.dataclass_inherited_transform(class, statement, variables);
        let decorated = statement.decorator_list.iter().find_map(|decorator| {
            let function = if let Expr::Call(call) = &decorator.expression {
                call.func.as_ref()
            } else {
                &decorator.expression
            };
            self.dataclass_transform_spec(function, variables)
                .map(|spec| (decorator, spec))
        });
        let (options, reason, spec, class_options) = if let Some((decorator, spec)) = decorated {
            let options = if let Expr::Call(call) = &decorator.expression {
                Some(&call.arguments)
            } else {
                None
            };
            (options, decorator.expression.range(), spec, false)
        } else if let Some(spec) = inherited {
            (
                statement.arguments.as_deref(),
                statement.range(),
                spec,
                true,
            )
        } else {
            return;
        };
        let neutral_frozen =
            self.dataclass_direct_transform_metaclass(self.class_id(class.instance).unwrap());
        let frozen = flag(options, "frozen", spec.frozen_default);
        let mut keyword_only = flag(options, "kw_only", spec.keyword_only_default);
        let mut sentinels = Vec::new();
        let mut duplicate_kw_only = Vec::new();
        let mut invalid_descriptors = Vec::new();
        let mut attributes = Vec::<Attribute>::new();
        let id = self.class_id(class.instance).unwrap();
        for parent in self.class_lineage(id).into_iter().skip(1).rev() {
            if let Some(definition) = &self.class(parent).dataclass {
                for sentinel in &definition.sentinels {
                    if !sentinels.contains(sentinel) {
                        sentinels.push(sentinel.clone());
                    }
                }
                for attribute in &definition.attributes {
                    let mut attribute = attribute.clone();
                    attribute.ty = self
                        .substitute(attribute.ty, &self.class_solutions(class.instance, parent));
                    if let Some(previous) = attributes
                        .iter_mut()
                        .find(|previous| previous.name == attribute.name)
                    {
                        *previous = attribute;
                    } else {
                        attributes.push(attribute);
                    }
                }
            }
        }
        for member in &statement.body {
            let Stmt::AnnAssign(assignment) = member else {
                continue;
            };
            let Expr::Name(name) = assignment.target.as_ref() else {
                continue;
            };
            let annotation = assignment.annotation.as_ref();
            if self.typing_form(annotation, variables).as_deref() == Some("KW_ONLY") {
                if !sentinels.is_empty() {
                    duplicate_kw_only.push(assignment.target.range());
                }
                sentinels.push(name.id.to_string());
                keyword_only = true;
                class.fields.remove(name.id.as_str());
                continue;
            }
            let form = self.typing_form(
                if let Expr::Subscript(subscript) = annotation {
                    &subscript.value
                } else {
                    annotation
                },
                variables,
            );
            if form.as_deref() == Some("ClassVar") {
                continue;
            }
            let field = assignment.value.as_deref().and_then(|value| {
                let Expr::Call(call) = value else {
                    return None;
                };
                self.typing_form(&call.func, variables)
                    .is_some_and(|form| spec.field_specifiers.contains(&form))
                    .then_some(call)
            });
            let required = if let Some(field) = field {
                !field.arguments.keywords.iter().any(|keyword| {
                    keyword.arg.as_ref().is_some_and(|name| {
                        matches!(name.as_str(), "default" | "default_factory" | "factory")
                    })
                })
            } else {
                assignment.value.is_none()
            };
            let declared = self
                .annotation_with_variables(annotation, variables)
                .unwrap_or(Type::Any);
            let ty = if required && !spec.transformed {
                declared
            } else {
                self.dataclass_descriptor_input(declared)
                    .unwrap_or_else(|message| {
                        invalid_descriptors.push((message, assignment.range()));
                        declared
                    })
            };
            let attribute = Attribute {
                name: name.id.to_string(),
                alias: spec
                    .transformed
                    .then(|| {
                        field.and_then(|call| {
                            call.arguments
                                .keywords
                                .iter()
                                .find(|keyword| {
                                    keyword
                                        .arg
                                        .as_ref()
                                        .is_some_and(|name| name.as_str() == "alias")
                                })
                                .and_then(|keyword| {
                                    if let Expr::StringLiteral(value) = &keyword.value {
                                        Some(value.value.to_str().to_owned())
                                    } else {
                                        None
                                    }
                                })
                        })
                    })
                    .flatten(),
                ty,
                required,
                in_init: self.dataclass_field_init(field, variables),
                keyword_only: flag(field.map(|call| &call.arguments), "kw_only", keyword_only),
                init_var: form.as_deref() == Some("InitVar"),
                neutral_frozen,
                range: assignment.target.range(),
            };
            if attribute.init_var {
                class.fields.remove(&attribute.name);
            }
            if spec.transformed {
                if let Some(field) = class.fields.get_mut(&attribute.name) {
                    field.class_access = true;
                }
            }
            if let Some(previous) = attributes
                .iter_mut()
                .find(|previous| previous.name == attribute.name)
            {
                *previous = attribute;
            } else {
                attributes.push(attribute);
            }
        }
        let mut has_default = false;
        let mut invalid_order = Vec::new();
        for attribute in attributes
            .iter()
            .filter(|attribute| attribute.in_init && !attribute.keyword_only)
        {
            if attribute.required && has_default {
                invalid_order.push(attribute.range);
            }
            has_default |= !attribute.required;
        }
        if frozen {
            for attribute in &attributes {
                if attribute.init_var || attribute.neutral_frozen {
                    continue;
                }
                if !class.fields.contains_key(&attribute.name) {
                    if let Some(field) = self.class_field(class.instance, &attribute.name) {
                        class.fields.insert(attribute.name.clone(), field);
                    }
                }
                if let Some(field) = class.fields.get_mut(&attribute.name) {
                    field.readonly = true;
                }
            }
        }
        if flag(options, "init", true) && !class.methods.contains_key("__init__") {
            self.dataclass_constructor(class, &attributes, variables);
        }
        let slots_conflict = self.dataclass_metadata(
            class,
            statement,
            options,
            &attributes,
            &sentinels,
            spec.transformed,
        );
        let generated_slots = flag(options, "slots", false)
            && !slots_conflict
            && class.fields.contains_key("__slots__");
        let order = flag(options, "order", spec.order_default);
        let invalid_eq = (order && !flag(options, "eq", spec.eq_default)).then_some(reason);
        let order_conflicts = if order {
            self.dataclass_order_methods(class, statement)
        } else {
            Vec::new()
        };
        let generated_replace = self.module_target(&class.namespace)
            != mamba_language::application::quality_syntax::PythonTarget::Py312;
        if generated_replace {
            self.dataclass_replace_method(class, &attributes, variables);
        }
        class.dataclass = Some(Definition {
            attributes,
            transformed: spec.transformed,
            field_specifiers: spec.field_specifiers,
            class_options,
            neutral_frozen,
            frozen,
            invalid_descriptors,
            invalid_order,
            duplicate_kw_only,
            slots_conflict,
            sentinels,
            generated_slots,
            order,
            invalid_eq,
            order_conflicts,
            generated_replace,
        });
    }
}
