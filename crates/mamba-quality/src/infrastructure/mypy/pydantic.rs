//! Built-in Pydantic model data. No Python plugin is imported or executed.
use super::types::{self, Type};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
use std::collections::{BTreeMap, BTreeSet};
mod class_properties;
mod computed_fields;
pub(in crate::infrastructure::mypy) mod configuration;
mod construct;
mod constructors;
pub(super) mod field_types;
mod imports;
mod info_members;
mod inheritance;
mod model_methods;
mod model_types;
pub(super) mod qualifiers;
mod root_models;

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub alias: Option<String>,
    pub ty: Option<Type>,
    pub required: bool,
    pub frozen: bool,
    pub dynamic_alias: bool,
}

impl Field {
    pub fn argument_name(&self, populate_by_name: bool) -> &str {
        if populate_by_name {
            &self.name
        } else {
            self.alias.as_deref().unwrap_or(&self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub frozen: bool,
    pub populate_by_name: bool,
    pub validate_by_name: bool,
    pub validate_by_alias: Option<bool>,
    pub alias_generator: bool,
    pub from_attributes: Option<bool>,
    pub is_root_model: bool,
    pub(super) configuration: configuration::Values,
}

impl Model {
    pub fn accepts_names(&self) -> bool {
        self.populate_by_name || self.validate_by_name
    }

    pub fn uses_aliases(&self) -> bool {
        !self.accepts_names() && self.validate_by_alias != Some(false)
    }

    pub fn allows_extra(&self) -> bool {
        !self.accepts_names()
            && self
                .fields
                .iter()
                .any(|field| field.dynamic_alias || (self.alias_generator && field.alias.is_none()))
    }

    pub fn optional_init_fields(&self) -> bool {
        self.alias_generator && !self.validate_by_name
    }

    pub fn init_fields(&self) -> impl Iterator<Item = &Field> {
        self.fields
            .iter()
            .filter(|field| !self.uses_aliases() || !field.dynamic_alias)
    }
}

fn name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => {
            Some(format!("{}.{}", name(&attribute.value)?, attribute.attr))
        }
        Expr::Subscript(subscript) => name(&subscript.value),
        _ => None,
    }
}

fn text(expression: &Expr) -> Option<String> {
    match expression {
        Expr::StringLiteral(string) => Some(string.value.to_str().to_string()),
        _ => None,
    }
}

fn flag(expression: &Expr) -> bool {
    matches!(expression, Expr::BooleanLiteral(boolean) if boolean.value)
}

fn field(
    statement: &Stmt,
    functions: &BTreeSet<String>,
    context: &types::Context,
    variables: &types::Variables,
) -> Option<Field> {
    let Stmt::AnnAssign(assignment) = statement else {
        return None;
    };
    let Expr::Name(identifier) = assignment.target.as_ref() else {
        return None;
    };
    let field_name = identifier.id.to_string();
    if field_name.starts_with('_') || field_name == "model_config" {
        return None;
    }
    if qualifiers::form(&assignment.annotation, context, variables).as_deref() == Some("ClassVar") {
        return None;
    }
    let mut field = Field {
        name: field_name,
        alias: None,
        ty: context.annotation_with_variables(&assignment.annotation, variables),
        required: assignment
            .value
            .as_deref()
            .is_none_or(|value| matches!(value, Expr::EllipsisLiteral(_))),
        frozen: false,
        dynamic_alias: false,
    };
    if let Some(Expr::Call(call)) = assignment.value.as_deref() {
        if name(&call.func).is_some_and(|value| functions.contains(&value)) {
            field.required = true;
            let mut has_default = false;
            if let Some(default) = call.arguments.args.first() {
                field.required = matches!(default, Expr::EllipsisLiteral(_));
                has_default = true;
            }
            for keyword in &call.arguments.keywords {
                match keyword.arg.as_ref().map(|name| name.as_str()) {
                    Some("default") if !has_default => {
                        field.required = matches!(keyword.value, Expr::EllipsisLiteral(_));
                        has_default = true;
                    }
                    Some("default_factory") if !has_default => {
                        field.required = matches!(keyword.value, Expr::NoneLiteral(_));
                        has_default = true;
                    }
                    Some("validation_alias") => {
                        field.alias = text(&keyword.value);
                        field.dynamic_alias = field.alias.is_none();
                    }
                    Some("alias")
                        if !call.arguments.keywords.iter().any(|keyword| {
                            keyword
                                .arg
                                .as_ref()
                                .is_some_and(|key| key.as_str() == "validation_alias")
                        }) =>
                    {
                        field.alias = text(&keyword.value);
                        field.dynamic_alias = field.alias.is_none();
                    }
                    Some("frozen") => field.frozen = flag(&keyword.value),
                    _ => {}
                }
            }
        }
    }
    if !field.required
        && qualifiers::form(&assignment.annotation, context, variables).as_deref() == Some("Final")
    {
        return None;
    }
    Some(field)
}

pub fn collect(
    statements: &[Stmt],
    context: &types::Context,
    variables: &types::Variables,
) -> BTreeMap<String, Model> {
    let (base_models, field_functions) = imports::symbols(statements);
    let mut models: BTreeMap<String, Model> = BTreeMap::new();
    for statement in statements {
        let Stmt::ClassDef(class) = statement else {
            continue;
        };
        let variables = variables
            .get(class.name.as_str())
            .and_then(|&ty| context.class_id(ty))
            .map_or_else(
                || variables.clone(),
                |id| context.class_scope_variables(id, variables.clone()),
            );
        let bases: Vec<_> = class
            .arguments
            .iter()
            .flat_map(|arguments| &arguments.args)
            .filter_map(name)
            .collect();
        let parents: Vec<_> = class
            .arguments
            .iter()
            .flat_map(|arguments| &arguments.args)
            .filter_map(|base| {
                name(base)
                    .and_then(|base| models.get(&base).cloned())
                    .or_else(|| {
                        context
                            .annotation_with_variables(base, &variables)
                            .and_then(|ty| context.class_id(ty))
                            .and_then(|id| {
                                context.class(id).pydantic.clone().or_else(|| {
                                    context
                                        .classes
                                        .borrow()
                                        .embedded_pydantic_models
                                        .get(&id)
                                        .cloned()
                                })
                            })
                    })
            })
            .collect();
        let declared_parent = class
            .arguments
            .iter()
            .flat_map(|arguments| &arguments.args)
            .any(|base| {
                name(base).is_some_and(|base| {
                    statements.iter().any(|statement|
                    matches!(statement, Stmt::ClassDef(parent) if parent.name.as_str() == base))
                }) && context
                    .annotation_with_variables(base, &variables)
                    .and_then(|ty| context.class_id(ty))
                    .is_some_and(|id| {
                        context
                            .class_lineage(id)
                            .iter()
                            .any(|&id| context.class(id).namespace == "pydantic.main.BaseModel")
                    })
            });
        if parents.is_empty()
            && !declared_parent
            && !bases.iter().any(|base| base_models.contains(base))
        {
            continue;
        }
        let mut model = Model {
            name: class.name.to_string(),
            fields: Vec::new(),
            frozen: false,
            populate_by_name: false,
            validate_by_name: false,
            validate_by_alias: None,
            alias_generator: false,
            from_attributes: None,
            is_root_model: false,
            configuration: configuration::local(class),
        };
        let instance = variables.get(class.name.as_str()).copied();
        model.fields =
            inheritance::inherited_fields(context, instance, &models, &variables, &parents);
        for parent in parents {
            model.configuration.inherit(&parent.configuration);
            model.is_root_model |= parent.is_root_model;
        }
        model.configuration.clone().apply(&mut model);
        let target = instance
            .and_then(|ty| context.class_id(ty))
            .map(|id| context.module_target(&context.class(id).namespace))
            .unwrap_or(mamba_language::application::quality_syntax::PythonTarget::Py312);
        let mut local_fields = std::collections::BTreeSet::new();
        for statement in super::classes::statements::collect(&class.body, target) {
            if let Some(field) = field(statement, &field_functions, context, &variables) {
                if !local_fields.insert(field.name.clone()) {
                    continue;
                }
                if let Some(previous) = model
                    .fields
                    .iter_mut()
                    .find(|previous| previous.name == field.name)
                {
                    *previous = field;
                } else {
                    model.fields.push(field);
                }
            }
        }
        if let Some(id) = variables
            .get(class.name.as_str())
            .and_then(|&ty| context.class_id(ty))
        {
            let ty = context.class(id).instance;
            for field in &mut model.fields {
                field.ty = context
                    .raw_class_field(ty, &field.name)
                    .and_then(|member| member.ty)
                    .or(field.ty);
            }
        }
        models.insert(model.name.clone(), model);
    }
    models
}
