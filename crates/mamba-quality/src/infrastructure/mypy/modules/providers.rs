//! Source exports bound the native providers that can supply their values.
use super::{Context, Export, Exports, Lookup, ModuleId, Type};
use crate::infrastructure::mypy::types::VariableMap;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_expr, walk_stmt, Visitor},
        Expr, ExprContext, Stmt, TypeParam,
    },
};
use std::collections::BTreeSet;

impl Lookup {
    pub(in crate::infrastructure::mypy) fn exports(&self, module: &str) -> Option<Exports> {
        let id = self.module_id(module)?;
        self.record_export_use(id);
        if self.modules.export_states.borrow()[id]
            .as_ref()
            .is_none_or(|state| !state.gaps.is_empty())
        {
            return None;
        }
        let exports = self.modules.exports.borrow()[id].clone();
        Some(
            exports
                .into_iter()
                .map(|(name, export)| {
                    let export = self.graph_export(id, module, &name, export);
                    (name, export)
                })
                .collect(),
        )
    }

    // Self-package from-imports prefer an actual graph module over a same-name value.
    pub(in crate::infrastructure::mypy) fn named_import_member(
        &self,
        module: &str,
        member: &str,
    ) -> Option<Export> {
        let child = format!("{module}.{member}");
        if module == self.modules.info[self.current].1.as_str() && self.module_id(&child).is_some()
        {
            return Some(Export {
                annotation_module: true,
                ty: None,
                callable: None,
                module: Some(child),
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        self.member(module, member)
    }

    pub(in crate::infrastructure::mypy) fn named_import_hidden(
        &self,
        module: &str,
        member: &str,
        export: &Export,
    ) -> Option<bool> {
        let fullname = format!("{module}.{member}");
        let current = &self.modules.info[self.current].1;
        if !fullname.starts_with(&format!("{current}.")) || export.module.is_none() {
            return Some(true);
        }
        let resolved = export.module.as_deref()?;
        if self.module_id(&fullname).is_some() && self.module_id(resolved).is_some() {
            Some(false)
        } else {
            // Provider assets do not record original module_hidden or all module nodes.
            None
        }
    }

    pub(in crate::infrastructure::mypy) fn member(
        &self,
        module: &str,
        member: &str,
    ) -> Option<Export> {
        if let Some(id) = self.module_id(module) {
            self.record_export_use(id);
            let export = self.modules.exports.borrow()[id].get(member).cloned();
            if let Some(export) = export {
                return Some(self.graph_export(id, module, member, export));
            }
            if self.modules.embedded_sources[id]
                && super::super::dependencies::embedded::preserves_bootstrap_members(module)
            {
                if let Some(export) = self
                    .native_member(module, member)
                    .or_else(|| self.standard_function_member(module, member))
                    .or_else(|| self.standard_module_member(module, member))
                {
                    return Some(export);
                }
            }
        } else if let Some(export) = self
            .native_member(module, member)
            .or_else(|| self.standard_function_member(module, member))
            .or_else(|| self.standard_module_member(module, member))
            .or_else(|| self.typed_child_member(module, member))
        {
            return Some(export);
        }
        let child = format!("{module}.{member}");
        (self.module_exists(&child)
            || (self.standard_module_exists(&child)
                && (!self.module_exists(module)
                    || self
                        .modules
                        .imported
                        .contains(&(self.modules.info[self.current].0, child.clone()))))
            || (self.typed_child_module_exists(&child)
                && crate::infrastructure::embedded::module(&child).is_some()
                && self
                    .modules
                    .imported
                    .contains(&(self.modules.info[self.current].0, child.clone())))
            || (!self.module_exists(module)
                && (Self::native_pydantic_module(&child)
                    || matches!(child.as_str(), "collections.abc" | "string.templatelib"))))
        .then_some(Export {
            annotation_module: self.annotation_module_exists(&child),
            ty: None,
            callable: None,
            module: Some(child),
            explicit: true,
            module_hidden: Some(false),
            alias: None,
        })
    }

    pub(in crate::infrastructure::mypy) fn annotation_paths(
        &self,
        body: &[Stmt],
        variables: &VariableMap,
    ) -> BTreeSet<String> {
        let mut paths = AnnotationPaths {
            context: &self.modules.context,
            variables,
            names: BTreeSet::new(),
            shadowed: BTreeSet::new(),
        };
        paths.visit_body(body);
        paths.names
    }

    pub(in crate::infrastructure::mypy) fn module_annotation(
        &self,
        module: &str,
        path: &str,
    ) -> Option<Type> {
        let mut module = module.to_owned();
        let mut members = path.split('.').peekable();
        while let Some(member) = members.next() {
            let export = self.member(&module, member)?;
            if members.peek().is_some() {
                module = export.module?;
            } else {
                return export
                    .ty
                    .filter(|&ty| self.modules.context.is_class_value(ty))
                    .map(|ty| self.modules.context.annotation_value(ty))
                    .or(export.alias);
            }
        }
        None
    }

    pub(in crate::infrastructure::mypy) fn annotation_module_exists(&self, module: &str) -> bool {
        self.module_id(module).is_some() || self.standard_module_exists(module)
    }

    pub(in crate::infrastructure::mypy) fn canonical_callable_annotation(
        &self,
        module: &str,
        path: &str,
    ) -> Option<Type> {
        let mut module = module.to_owned();
        let mut members = path.split('.').peekable();
        while let Some(member) = members.next() {
            let export = self.member(&module, member)?;
            if members.peek().is_some() {
                if !export.annotation_module {
                    return None;
                }
                module = export.module?;
            } else {
                return export
                    .alias
                    .filter(|&ty| self.modules.context.is_callable_form(ty));
            }
        }
        None
    }

    // Graph names and visibility stay source-owned, even for native intrinsics.
    fn graph_export(&self, id: ModuleId, module: &str, member: &str, mut export: Export) -> Export {
        if self.modules.embedded_sources[id] {
            if let Some(native) = self.native_member_for(module, member, true) {
                export.annotation_module = export.annotation_module
                    && native.annotation_module
                    && export.module == native.module;
                export.ty = native.ty;
                export.callable = native.callable;
                export.alias = native.alias;
            }
            if export.module.is_none()
                && matches!(module, "typing" | "typing_extensions")
                && member == "Callable"
            {
                if let Some(alias) = self
                    .standard_module_member(module, member)
                    .and_then(|export| export.alias)
                    .filter(|&alias| self.modules.context.is_callable_form(alias))
                {
                    export.alias = Some(alias);
                }
            }
        }
        export
    }
}

struct AnnotationPaths<'a> {
    context: &'a Context,
    variables: &'a VariableMap,
    names: BTreeSet<String>,
    shadowed: BTreeSet<String>,
}

impl AnnotationPaths<'_> {
    fn collect(&mut self, expression: &Expr) {
        match expression {
            Expr::Attribute(_) => {
                if let Some(name) = dotted_name(expression) {
                    if !self.shadowed.contains(name.split('.').next().unwrap()) {
                        self.names.insert(name);
                    }
                }
            }
            Expr::StringLiteral(string) => {
                let source = format!("({})", string.value.to_str());
                if let Ok(document) =
                    quality_syntax::parse(&source, quality_syntax::PythonTarget::Py314, false)
                {
                    if document.is_valid() {
                        if let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() {
                            if !matches!(statement.value.as_ref(), Expr::StringLiteral(_)) {
                                self.collect(&statement.value);
                            }
                        }
                    }
                }
            }
            Expr::Subscript(subscript) => {
                self.collect(&subscript.value);
                if self
                    .context
                    .is_literal_form(&subscript.value, self.variables)
                {
                    return;
                }
                if self
                    .context
                    .typing_form(&subscript.value, self.variables)
                    .as_deref()
                    == Some("Annotated")
                {
                    if let Expr::Tuple(tuple) = subscript.slice.as_ref() {
                        if let Some(value) = tuple.elts.first() {
                            self.collect(value);
                        }
                        return;
                    }
                }
                self.collect(&subscript.slice);
            }
            Expr::BinOp(binary) => {
                self.collect(&binary.left);
                self.collect(&binary.right);
            }
            Expr::Tuple(tuple) => tuple.elts.iter().for_each(|value| self.collect(value)),
            Expr::List(list) => list.elts.iter().for_each(|value| self.collect(value)),
            Expr::Starred(starred) => self.collect(&starred.value),
            _ => {}
        }
    }
}

impl<'ast> Visitor<'ast> for AnnotationPaths<'_> {
    fn visit_annotation(&mut self, expression: &'ast Expr) {
        self.collect(expression);
    }

    fn visit_expr(&mut self, _expression: &'ast Expr) {}

    fn visit_type_param(&mut self, parameter: &'ast TypeParam) {
        if let TypeParam::TypeVar(parameter) = parameter {
            if let Some(bound) = &parameter.bound {
                self.collect(bound);
            }
        }
        if let Some(default) = parameter.default() {
            self.collect(default);
        }
    }

    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let previous = matches!(
            statement,
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) | Stmt::TypeAlias(_)
        )
        .then(|| self.shadowed.clone());
        let parameters = match statement {
            Stmt::FunctionDef(function) => function.type_params.as_deref(),
            Stmt::ClassDef(class) => class.type_params.as_deref(),
            Stmt::TypeAlias(alias) => alias.type_params.as_deref(),
            _ => None,
        };
        if let Some(parameters) = parameters {
            self.shadowed.extend(
                parameters
                    .type_params
                    .iter()
                    .map(|parameter| parameter.name().to_string()),
            );
            self.visit_type_params(parameters);
        }
        match statement {
            Stmt::FunctionDef(function) => {
                self.visit_parameters(&function.parameters);
                if let Some(returns) = &function.returns {
                    self.collect(returns);
                }
            }
            Stmt::ClassDef(class) => {
                if let Some(arguments) = &class.arguments {
                    for base in &arguments.args {
                        self.collect(base);
                    }
                    for keyword in &arguments.keywords {
                        if keyword
                            .arg
                            .as_ref()
                            .is_some_and(|name| name.as_str() == "metaclass")
                        {
                            self.collect(&keyword.value);
                        }
                    }
                }
                let mut bound = ClassBindings::default();
                bound.visit_body(&class.body);
                self.shadowed.extend(bound.0);
                self.visit_body(&class.body);
            }
            Stmt::TypeAlias(alias) => self.collect(&alias.value),
            Stmt::AnnAssign(assignment) => {
                self.collect(&assignment.annotation);
                if self
                    .context
                    .typing_form(&assignment.annotation, self.variables)
                    .as_deref()
                    == Some("TypeAlias")
                {
                    if let Some(value) = &assignment.value {
                        self.collect(value);
                    }
                }
            }
            Stmt::Assign(assignment)
                if matches!(assignment.targets.as_slice(), [Expr::Name(_)])
                    && matches!(
                        assignment.value.as_ref(),
                        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::BinOp(_)
                    ) =>
            {
                self.collect(&assignment.value)
            }
            _ => walk_stmt(self, statement),
        }
        if let Some(previous) = previous {
            self.shadowed = previous;
        }
    }
}

#[derive(Default)]
struct ClassBindings(BTreeSet<String>);

impl<'ast> Visitor<'ast> for ClassBindings {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                self.0.insert(function.name.to_string());
            }
            Stmt::ClassDef(class) => {
                self.0.insert(class.name.to_string());
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in crate::infrastructure::project::imports(
                    std::slice::from_ref(statement),
                    "",
                    false,
                ) {
                    self.0.insert(import.local);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if matches!(
            expression,
            Expr::Lambda(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
        ) {
            return;
        }
        if let Expr::Name(name) = expression {
            if name.ctx == ExprContext::Store {
                self.0.insert(name.id.to_string());
            }
        }
        walk_expr(self, expression);
    }
}

fn dotted_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            dotted_name(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}
