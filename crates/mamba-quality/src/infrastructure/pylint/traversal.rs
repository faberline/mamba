//! Default lexical and import diagnostics use Pylint's own lookup decisions.
use super::Checker;
use crate::infrastructure::project::Import;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Comprehension, ExceptHandler, Expr, Pattern, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

impl Checker<'_> {
    pub(super) fn leave_scope(&mut self) {
        self.finish_resource_scope();
        let threading_module = self.module_name == "threading" && self.scopes.current_index() == 0;
        let function_scope = self.scopes.current_index() > 0;
        let mut wildcard: BTreeMap<(String, u32, u32), Vec<String>> = BTreeMap::new();
        let mut checked = std::collections::BTreeSet::new();
        for import in self.scopes.leave() {
            if threading_module && import.local == "Lock" {
                continue;
            }
            if import.member.as_deref() == Some("*") {
                wildcard
                    .entry((
                        import.module,
                        import.range.start().to_u32(),
                        import.range.end().to_u32(),
                    ))
                    .or_default()
                    .push(import.local);
                continue;
            }
            if !checked.insert(if function_scope {
                import.local.clone()
            } else {
                import
                    .member
                    .as_deref()
                    .unwrap_or(&import.module)
                    .to_string()
            }) {
                continue;
            }
            let message = match &import.member {
                Some(member) => format!(
                    "Unused {member} imported from {}{}",
                    self.import_message_module(&import),
                    import
                        .alias
                        .as_ref()
                        .map(|alias| format!(" as {alias}"))
                        .unwrap_or_default()
                ),
                None => import.alias.as_ref().map_or_else(
                    || format!("Unused import {}", import.module),
                    |alias| format!("Unused {} imported as {alias}", import.module),
                ),
            };
            self.emit("W0611", "unused-import", &message, import.range);
        }
        for ((module, start, end), names) in wildcard {
            self.emit(
                "W0614",
                "unused-wildcard-import",
                &format!(
                    "Unused import(s) {} from wildcard import of {module}",
                    names.join(", ")
                ),
                TextRange::new(start.into(), end.into()),
            );
        }
    }

    pub(super) fn check_import(&mut self, import: Import) {
        if import.module == "logging" && import.member.is_none() {
            self.logging_names.insert(import.local.clone());
        }
        if let Some(error) = self
            .modules
            .import_failure(self.source.project, &import.module)
        {
            self.failed_imports.insert(import.module.clone());
            let start = import.range.start().to_usize();
            let line = self.source.text[..start]
                .rfind('\n')
                .map_or(0, |offset| offset + 1);
            self.emit(
                "E0001",
                "syntax-error",
                &format!(
                    "Cannot import '{}' due to '{error}'",
                    self.import_message_module(&import)
                ),
                super::super::diagnostics::point(line),
            );
            if !self.relative_import(import.range) {
                self.check_import_module_members(&import);
            }
            return;
        }
        if self.guarded_imports || self.type_checking {
            return;
        }
        if import.module.is_empty() {
            if let Some(member) = &import.member {
                self.emit(
                    "E0611",
                    "no-name-in-module",
                    &format!("No name '{member}' in module ''"),
                    import.range,
                );
            }
            return;
        }
        if self.modules.exists(self.source.project, &import.module)
            && import.module == self.module_name
        {
            self.emit("W0406", "import-self", "Module import itself", import.range);
        }
        if !self.modules.exists(self.source.project, &import.module) {
            self.failed_imports.insert(import.module.clone());
            self.emit(
                "E0401",
                "import-error",
                &format!("Unable to import '{}'", import.module),
                import.range,
            );
        }
        self.check_import_module_members(&import);
    }

    pub(super) fn module_expression(&self, expression: &Expr) -> Option<String> {
        match self.callable_target(expression)? {
            super::callables::Target::Module(name) => Some(name),
            _ => None,
        }
    }

    pub(super) fn comprehension_body<'a>(
        &mut self,
        generators: &'a [Comprehension],
        values: &[&'a Expr],
    ) {
        if let Some(first) = generators.first() {
            self.visit_expr(&first.iter);
        }
        for generator in generators {
            let mut names = super::local_rules::target_names(&generator.target);
            for name in names.drain(..) {
                self.scopes.use_name(&name);
            }
        }
        self.scopes.enter_expression(None);
        for (index, generator) in generators.iter().enumerate() {
            if index != 0 {
                self.visit_expr(&generator.iter);
            }
            self.scopes.pattern(&generator.target);
            if let Expr::Name(name) = &generator.target {
                self.check_bad_name(name.id.as_str(), name.range());
            }
            for condition in &generator.ifs {
                self.check_constant_condition(condition, generator.range());
                self.visit_expr(condition);
            }
        }
        for value in values {
            self.visit_expr(value);
        }
        for generator in generators {
            if let Expr::Name(name) = &generator.target {
                self.check_non_ascii_name(name.id.as_str(), "Variable", name.range());
            }
        }
        self.check_indexed_comprehensions(generators, values);
        self.leave_scope();
    }
}

impl<'a> Visitor<'a> for Checker<'_> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        for statement in body {
            if !self.statements.docs.contains(&statement.start().to_u32()) {
                self.visit_stmt(statement);
            }
        }
    }

    fn visit_stmt(&mut self, statement: &'a Stmt) {
        self.check_multiple_statements(statement);
        self.check_flow_context(statement);
        match statement {
            Stmt::FunctionDef(function) => self.walk_function_declaration(function),
            Stmt::ClassDef(class) => self.walk_class_declaration(class),
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                self.check_imports(statement);
            }
            Stmt::Assign(assignment) => self.walk_assignment(assignment),
            Stmt::TypeAlias(alias) => {
                self.enter_generic_names(alias.type_params.as_deref());
                walk_stmt(self, statement);
                self.generic_names.pop();
            }
            Stmt::Global(global) => self.check_global(global),
            Stmt::AugAssign(assignment) => {
                self.check_assignment_slots(&assignment.target);
                self.check_join_assignment(assignment);
                self.check_assignment_name(&assignment.target, &assignment.value);
                let previous = self.augmented_assignment;
                self.augmented_assignment = true;
                walk_stmt(self, statement);
                self.augmented_assignment = previous;
            }
            Stmt::AnnAssign(assignment) => {
                self.check_annotated_assignment_name(assignment);
                let direct = self
                    .statements
                    .contexts
                    .get(&assignment.start().to_u32())
                    .is_some_and(|context| context.parent == super::statements::Kind::Function);
                self.check_non_ascii_target(&assignment.target, direct);
                self.visit_expr(&assignment.target);
                let previous = self.annotation_context;
                self.annotation_context = true;
                self.visit_expr(&assignment.annotation);
                self.annotation_context = previous;
                if let Some(value) = &assignment.value {
                    self.visit_expr(value);
                }
            }
            Stmt::Nonlocal(_) => self.check_semantic_statement(statement),
            Stmt::Pass(_) | Stmt::Expr(_) => {
                self.check_basic_statement(statement);
                walk_stmt(self, statement);
            }
            Stmt::If(conditional) => {
                self.check_constant_condition(&conditional.test, conditional.range());
                self.check_refactoring_if(conditional);
                self.check_else_return(conditional);
                self.check_boolean_design(&conditional.test);
                self.visit_expr(&conditional.test);
                let previous_import_guard = self.guarded_imports;
                self.guarded_imports |= super::import_rules::sys_guard(&conditional.test);
                let previous = self.type_checking;
                self.type_checking |= matches!(conditional.test.as_ref(), Expr::Name(name) if name.id.as_str() == "TYPE_CHECKING")
                    || matches!(conditional.test.as_ref(), Expr::Attribute(attr) if attr.attr.as_str() == "TYPE_CHECKING");
                self.visit_body(&conditional.body);
                self.type_checking = previous;
                self.guarded_imports = previous_import_guard;
                for clause in &conditional.elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            Stmt::Assert(assertion) => {
                self.check_assertion(assertion);
                walk_stmt(self, statement);
            }
            Stmt::With(value) => {
                self.use_resource_with(value);
                self.check_confusing_with(value);
                self.check_context_manager(value);
                self.check_threading_with(value);
                for item in &value.items {
                    self.resources.suppressed += 1;
                    self.visit_expr(&item.context_expr);
                    self.resources.suppressed -= 1;
                    if let Some(target) = &item.optional_vars {
                        self.visit_expr(target);
                    }
                    self.bind_context_value(item);
                }
                self.visit_body(&value.body);
            }
            Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_) => {
                self.check_unreachable(statement);
                self.check_semantic_statement(statement);
                if let Stmt::Raise(raised) = statement {
                    self.check_exception_raise(raised);
                    self.check_stop_raise(raised);
                }
                if let Stmt::Return(returned) = statement {
                    if let Some(value) = &returned.value {
                        self.check_choice(value, statement.range());
                    }
                }
                self.resources.returned += usize::from(matches!(statement, Stmt::Return(_)));
                walk_stmt(self, statement);
                self.resources.returned -= usize::from(matches!(statement, Stmt::Return(_)));
            }
            Stmt::Try(attempt) => {
                self.check_exception_handlers(attempt);
                self.check_nested_block(statement);
                self.check_try_else(attempt);
                let previous = self.guarded_imports;
                self.guarded_imports |= attempt.handlers.iter().any(|handler| {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    matches!(handler.type_.as_deref(), Some(Expr::Name(name)) if matches!(name.id.as_str(), "ImportError" | "ModuleNotFoundError" | "Exception" | "BaseException"))
                });
                self.visit_body(&attempt.body);
                self.guarded_imports = previous;
                for handler in &attempt.handlers {
                    self.visit_except_handler(handler);
                }
                self.visit_body(&attempt.orelse);
                self.visit_body(&attempt.finalbody);
            }
            Stmt::For(loop_) => {
                self.enter_repeated_iterator_loop(loop_);
                self.check_repeated_target(&loop_.target);
                self.check_nested_block(statement);
                self.check_indexed_iteration(loop_);
                self.check_rebound_argument(&loop_.target);
                self.check_loop_else(&loop_.body, &loop_.orelse);
                self.check_dictionary_unpacking(loop_);
                self.check_modified_iteration(loop_);
                self.check_async_iteration(loop_);
                self.check_dictionary_missing_items(loop_);
                self.check_iteration_recommendations(loop_);
                walk_stmt(self, statement);
                if !loop_.is_async {
                    self.repeated_iterators.leave();
                }
            }
            Stmt::While(loop_) => {
                self.check_loop_else(&loop_.body, &loop_.orelse);
                self.check_nested_block(statement);
                walk_stmt(self, statement);
            }
            Stmt::Match(value) => {
                self.check_match_captures(value);
                walk_stmt(self, statement);
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        self.visit_expression(expression);
    }
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if let Pattern::MatchClass(value) = pattern {
            self.check_match_class(value);
        }
        mamba_language::application::quality_syntax::python_ast::visitor::walk_pattern(
            self, pattern,
        );
    }
    fn visit_type_param(
        &mut self,
        parameter: &'a mamba_language::application::quality_syntax::python_ast::TypeParam,
    ) {
        self.check_generic_parameter(parameter);
        mamba_language::application::quality_syntax::python_ast::visitor::walk_type_param(
            self, parameter,
        );
    }
    fn visit_elif_else_clause(
        &mut self,
        clause: &'a mamba_language::application::quality_syntax::python_ast::ElifElseClause,
    ) {
        if let Some(test) = &clause.test {
            self.check_boolean_design(test);
            self.check_nested_elif(clause);
            self.check_dictionary_get(clause.start().to_u32());
        }
        mamba_language::application::quality_syntax::python_ast::visitor::walk_elif_else_clause(
            self, clause,
        );
    }
}
