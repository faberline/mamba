//! Class design metrics retain original ownership and declaration timing.
use super::{
    callables::Target, classes::Class, defaults, resolution::Context, values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

fn decorator_function(expression: &Expr) -> &Expr {
    if let Expr::Call(call) = expression {
        &call.func
    } else {
        expression
    }
}

impl Checker<'_> {
    fn dataclass_decorator(&self, expression: &Expr) -> bool {
        match decorator_function(expression) {
            Expr::Name(name) => matches!(
                self.scopes.call_target_in(name.id.as_str(), self.scopes.current_index()),
                Some((_, Target::Imported(import))) if import.module == "dataclasses" && import.member.as_deref() == Some("dataclass")
            ),
            Expr::Attribute(attribute) if attribute.attr.as_str() == "dataclass" => {
                matches!(self.callable_target(&attribute.value), Some(Target::Module(module)) if module == "dataclasses")
            }
            _ => false,
        }
    }

    pub(super) fn add_dataclass_design_fields(&self, statement: &StmtClassDef, class: &mut Class) {
        if !statement
            .decorator_list
            .iter()
            .any(|decorator| self.dataclass_decorator(&decorator.expression))
        {
            return;
        }
        for statement in &statement.body {
            if let Stmt::AnnAssign(assignment) = statement {
                let Expr::Name(name) = assignment.target.as_ref() else {
                    continue;
                };
                let special = match assignment.annotation.as_ref() {
                    Expr::Subscript(subscript) => super::values::path(&subscript.value)
                        .and_then(|names| names.last().cloned()),
                    _ => None,
                };
                if special
                    .as_deref()
                    .is_some_and(|name| name == "ClassVar" || name == "InitVar")
                {
                    continue;
                }
                class
                    .attributes
                    .entry(name.id.to_string())
                    .or_insert_with(|| {
                        Target::Value(Rc::new(
                            assignment
                                .value
                                .as_deref()
                                .map(Value::from_expression)
                                .unwrap_or(Value::Unknown),
                        ))
                    });
            }
        }
    }

    pub(super) fn check_class_design(
        &mut self,
        statement: &StmtClassDef,
        class: Rc<Class>,
        context: Context,
    ) {
        let (max_parents, max_attributes) = defaults::fixed().design_limits();
        let mut seen = BTreeSet::from([class.identity.clone()]);
        let mut pending = vec![(class.clone(), context)];
        let mut parents = 0;
        while let Some((class, origin)) = pending.pop() {
            for base in &class.bases {
                if let Some((base, context)) = self.inferred_base(base, origin.clone()) {
                    let qualified = format!("{}.{}", base.identity.0, base.name);
                    if !defaults::fixed().ignored_parent(&qualified)
                        && seen.insert(base.identity.clone())
                    {
                        parents += 1;
                        pending.push((base, context));
                    }
                }
            }
        }
        let range = self.declaration_range(statement.name.range());
        if parents > max_parents {
            self.emit(
                "R0901",
                "too-many-ancestors",
                &format!("Too many ancestors ({parents}/{max_parents})"),
                range,
            );
        }
        let count = class
            .attributes
            .keys()
            .filter(|name| {
                class
                    .attribute_definitions
                    .iter()
                    .find(|definition| definition.name == ***name)
                    .is_none_or(|definition| {
                        definition.module.as_deref().unwrap_or(&class.identity.0)
                            == self.module_name
                    })
            })
            .count();
        if count > max_attributes {
            self.emit(
                "R0902",
                "too-many-instance-attributes",
                &format!("Too many instance attributes ({count}/{max_attributes})"),
                range,
            );
        }
    }

    pub(super) fn exempt_public_minimum(
        &self,
        statement: &StmtClassDef,
        class: Rc<Class>,
        origin: Context,
    ) -> bool {
        if self.enum_subtype(class.clone(), origin.clone(), "Enum")
            && !(class.identity.0 == "enum" && class.name == "Enum")
        {
            return true;
        }
        if self.lineage(class, origin).iter().any(|(class, _)| {
            class.identity.0 == "builtins"
                && matches!(class.name.as_str(), "type" | "BaseException")
        }) {
            return true;
        }
        let root = self.scopes.root_declarations();
        statement.decorator_list.iter().any(|decorator| {
            let Some(name) = super::values::path(decorator_function(&decorator.expression))
                .and_then(|names| names.last().cloned())
            else {
                return false;
            };
            defaults::fixed().data_decorator(&name)
                && root.keys().any(|name| {
                    defaults::fixed().data_decorator(name)
                        || matches!(name.as_str(), "dataclasses" | "attrs" | "attr")
                })
        })
    }

    pub(super) fn check_attribute_definitions(&mut self, class: Rc<Class>, origin: Context) {
        if defaults::fixed().ignored_mixin(&class.name, "attribute-defined-outside-init") {
            return;
        }
        let mut grouped: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        let calls = self.setattr_definitions(class.clone(), origin.clone());
        for definition in &class.attribute_definitions {
            if definition
                .module
                .as_deref()
                .is_some_and(|module| module != self.module_name)
            {
                continue;
            }
            if self.build_attributes.augmented(&class, definition.range) {
                continue;
            }
            grouped
                .entry(&definition.name)
                .or_default()
                .push(definition);
        }
        for definition in &calls {
            grouped
                .entry(&definition.name)
                .or_default()
                .push(definition);
        }
        let mut groups: Vec<_> = grouped.into_iter().collect();
        groups.sort_by_key(|(_, definitions)| definitions[0].range.start());
        let lineage = self.lineage(class.clone(), origin.clone());
        for (name, definitions) in groups {
            if name == "__dict__"
                || definitions.iter().any(|definition| {
                    definition.setter
                        || defaults::fixed().defining_method(&definition.method)
                        || self
                            .initialized_methods
                            .get(&class.identity)
                            .is_some_and(|methods| methods.contains(&definition.method))
                })
            {
                continue;
            }
            if lineage.iter().any(|(owner, context)| {
                self.local_class_member(owner, context.clone(), name, false)
                    .is_some()
            }) || lineage.iter().skip(1).any(|(parent, _)| {
                parent.attribute_definitions.iter().any(|definition| {
                    definition.name == name
                        && (definition.setter
                            || defaults::fixed().defining_method(&definition.method)
                            || self
                                .initialized_methods
                                .get(&parent.identity)
                                .is_some_and(|methods| methods.contains(&definition.method)))
                })
            }) {
                continue;
            }
            for definition in definitions {
                self.emit(
                    "W0201",
                    "attribute-defined-outside-init",
                    &format!("Attribute '{name}' defined outside __init__"),
                    definition.range,
                );
            }
        }
    }
}
