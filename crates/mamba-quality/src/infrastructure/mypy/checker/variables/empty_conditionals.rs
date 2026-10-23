//! Empty conditional operands retain their local bottom bounds until recovery.
use super::*;
use quality_syntax::python_ast::CmpOp;

impl Checker<'_> {
    pub(super) fn inferred_empty_conditional_assignment(
        &self,
        assignment: &StmtAssign,
        observed: Option<types::Type>,
    ) -> Option<types::Type> {
        if !self.flow.reachable {
            return None;
        }
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return None;
        };
        // This query admits only the first inferred definition in this function.
        // Declarations, redirects, outer bindings, and later writes keep their path.
        if name.id.as_str() == "_"
            || self.scopes.assigned_type(&assignment.targets[0]).is_some()
            || self
                .scopes
                .first_inferred_local_assignment(&assignment.targets[0])?
                .1
                != name.start()
        {
            return None;
        }
        let Expr::If(conditional) = assignment.value.as_ref() else {
            return None;
        };
        if super::super::super::classes::statements::truth(&conditional.test, self.python_target)
            .is_some()
        {
            return None;
        }
        // Prove both outcomes only for ordinary string equality. A two-arm
        // observed union alone does not establish semantic reachability.
        let Expr::Compare(compare) = conditional.test.as_ref() else {
            return None;
        };
        let operands: &[Expr] = compare.operands.as_ref();
        let operators: &[CmpOp] = compare.ops.as_ref();
        let [value @ Expr::Name(_), Expr::StringLiteral(_)] = operands else {
            return None;
        };
        if !matches!(operators, [CmpOp::Eq | CmpOp::NotEq])
            || !matches!(
                self.infer(value),
                Some(types::Type::Str | types::Type::Any | types::Type::AnySpecial)
            )
        {
            return None;
        }
        self.context.empty_conditional_collection_join(
            &assignment.value,
            observed?,
            self.python_target,
        )
    }
}
