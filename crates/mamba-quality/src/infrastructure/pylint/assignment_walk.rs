//! Assignments preserve checker order before binding target names.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::Visitor, Expr, Stmt, StmtAssign,
};
use ruff_text_size::Ranged;

fn valid_type_alias(name: &str) -> bool {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let unprefixed = name.trim_start_matches('_');
    !name.starts_with("___")
        && !unprefixed.starts_with("Type")
        && !unprefixed
            .as_bytes()
            .get(1)
            .is_some_and(|second| unprefixed.starts_with('T') && second.is_ascii_uppercase())
        && PATTERN
            .get_or_init(|| {
                regex::Regex::new(r"^[A-Z]+[a-z0-9]+(?:[A-Z][a-z0-9]+)*$")
                    .expect("locked Pylint type alias name regex")
            })
            .is_match(unprefixed)
}

impl Checker<'_> {
    fn imported_function_alias(&self, value: &Expr) -> bool {
        self.stdlib_name(value)
            .and_then(|name| {
                let (module, member) = name.rsplit_once('.')?;
                Some(
                    self.modules
                        .naming_function(self.source.project, module, member),
                )
            })
            .unwrap_or(false)
    }

    fn mark_initializing_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) if self.scopes.first_assignment(name.id.as_str(), name.range()) => {
                self.initializing_tuple.insert(name.id.to_string());
            }
            Expr::Tuple(value) => {
                for target in &value.elts {
                    self.mark_initializing_target(target);
                }
            }
            Expr::List(value) => {
                for target in &value.elts {
                    self.mark_initializing_target(target);
                }
            }
            Expr::Starred(value) => self.mark_initializing_target(&value.value),
            _ => {}
        }
    }

    pub(super) fn check_assignment_name(&mut self, target: &Expr, value: &Expr) {
        let Expr::Name(target) = target else {
            return;
        };
        if self.inherited_class_name(target.id.as_str()) {
            return;
        }
        if (self.scopes.function_local(target.id.as_str()) || self.scopes.in_class())
            && self.check_bad_name(target.id.as_str(), target.range())
        {
            return;
        }
        if self.scopes.in_class()
            && self.class_stack.last().is_some_and(|(class, origin)| {
                self.enum_member(class.clone(), origin.clone(), target.id.as_str())
            })
        {
            if !super::defaults::fixed().good(target.id.as_str())
                && !super::defaults::fixed().valid_name(target.id.as_str(), "const")
            {
                self.emit(
                    "C0103",
                    "invalid-name",
                    &format!(
                        "Class constant name \"{}\" doesn't conform to UPPER_CASE naming style",
                        target.id
                    ),
                    target.range(),
                );
            }
            return;
        }
        if self.scopes.function_local(target.id.as_str()) {
            let class_value = matches!(
                self.callable_target(value),
                Some(super::callables::Target::Class(_))
            ) || matches!(
                self.stdlib_name(value).as_deref(),
                Some(
                    "typing.TypeVar"
                        | "typing.ParamSpec"
                        | "typing.TypeVarTuple"
                        | "typing_extensions.TypeVar"
                        | "typing_extensions.ParamSpec"
                        | "typing_extensions.TypeVarTuple"
                )
            ) || matches!(value, Expr::Call(call) if matches!(self.stdlib_name(&call.func).as_deref(), Some("typing.TypeVar" | "typing.ParamSpec" | "typing.TypeVarTuple" | "typing_extensions.TypeVar" | "typing_extensions.ParamSpec" | "typing_extensions.TypeVarTuple")));
            if !class_value {
                self.check_declaration_name(target.id.as_str(), "variable", target.range());
            }
            return;
        }
        if self.scopes.is_module() && self.union_type_alias(value) {
            let valid = valid_type_alias(target.id.as_str());
            if !valid && !super::defaults::fixed().good(target.id.as_str()) {
                self.emit(
                    "C0103",
                    "invalid-name",
                    &format!(
                        "Type alias name \"{}\" doesn't conform to predefined naming style",
                        target.id
                    ),
                    target.range(),
                );
            }
            return;
        }
        if matches!(
            self.stdlib_name(value).as_deref(),
            Some(
                "threading.Thread"
                    | "typing.TypeVar"
                    | "typing.ParamSpec"
                    | "typing.TypeVarTuple"
                    | "typing_extensions.TypeVar"
                    | "typing_extensions.ParamSpec"
                    | "typing_extensions.TypeVarTuple"
            )
        ) || matches!(
            self.callable_target(value),
            Some(super::callables::Target::Class(_))
        ) {
            if self.scopes.is_module() {
                self.check_declaration_name(target.id.as_str(), "class", target.range());
            }
            return;
        }
        let function = matches!(value, Expr::Lambda(_))
            || matches!(
                self.callable_target(value),
                Some(super::callables::Target::Function(_))
            )
            || self.imported_function_alias(value)
            || matches!(value, Expr::Name(value) if self.modules.function(value.id.as_str()) || self.scopes.function(value.id.as_str()));
        if function && !super::defaults::fixed().valid_name(target.id.as_str(), "variable") {
            self.emit(
                "C0103",
                "invalid-name",
                &format!(
                    "Variable name \"{}\" doesn't conform to snake_case naming style",
                    target.id
                ),
                target.range(),
            );
        }
    }

    pub(super) fn walk_assignment(&mut self, assignment: &StmtAssign) {
        self.track_repeated_iterator(assignment);
        self.check_semantic_statement(&Stmt::Assign(assignment.clone()));
        self.check_match_definition(assignment);
        for target in &assignment.targets {
            self.check_same_assignment(target, &assignment.value);
            self.check_repeated_target(target);
        }
        self.check_receiver_assignment(assignment);
        self.check_unpacking(assignment);
        for target in &assignment.targets {
            self.check_assignment_slots(target);
        }
        self.check_string_assignment(&assignment.value);
        self.check_call_assignment(assignment);
        self.track_resource_assignment(assignment);
        self.check_dunder_name_assignment(assignment);
        if self.swaps.contains(&assignment.start().to_u32()) {
            self.emit(
                "R1712",
                "consider-swap-variables",
                "Consider using tuple unpacking for swapping variables",
                assignment.range(),
            );
        }
        self.check_method_wrapper(assignment);
        self.check_choice(&assignment.value, assignment.range());
        if let Some(Expr::Attribute(attribute)) = assignment.targets.first() {
            self.check_protected_access(attribute);
        }
        let typevars = self.check_typevar_assignment(assignment);
        for target in &assignment.targets {
            if !typevars.contains(&target.start().to_u32()) {
                self.check_assignment_name(target, &assignment.value);
                self.check_literal_name(target, &assignment.value);
            }
        }
        let direct = self
            .statements
            .contexts
            .get(&assignment.start().to_u32())
            .is_some_and(|context| context.parent == super::statements::Kind::Function);
        for target in &assignment.targets {
            self.check_non_ascii_target(target, direct);
        }
        let initializing = self.initializing.clone();
        let initializing_tuple = self.initializing_tuple.clone();
        for target in &assignment.targets {
            if let Expr::Name(name) = target {
                if self.scopes.first_assignment(name.id.as_str(), name.range()) {
                    self.initializing.insert(name.id.to_string());
                }
            } else {
                self.mark_initializing_target(target);
            }
        }
        let previous_property = self.property_assignment;
        self.property_assignment = self.is_property_assignment(assignment);
        self.visit_expr(&assignment.value);
        self.property_assignment = previous_property;
        self.initializing = initializing;
        self.initializing_tuple = initializing_tuple;
        for target in &assignment.targets {
            self.visit_expr(target);
        }
    }
}
