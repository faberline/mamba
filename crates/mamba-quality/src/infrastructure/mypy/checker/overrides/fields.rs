//! Method replacements compare their signatures with inherited field values.
use super::*;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_inherited_field_method(
        &mut self,
        function: &StmtFunctionDef,
        id: u32,
        method: &Method,
    ) {
        for base in self.context.class_lineage(id).into_iter().skip(1) {
            let parent = self.context.class(base);
            let Some(expected) = parent.fields.get(function.name.as_str()).and_then(|f| f.ty)
            else {
                continue;
            };
            let expected = self.context.substitute(
                expected,
                &self
                    .context
                    .class_solutions(self.context.class(id).instance, base),
            );
            let actual = self.context.callable_type(std::sync::Arc::new(
                method
                    .signature
                    .bound(usize::from(method.kind != MethodKind::Static)),
            ));
            if !self.context.accepts(expected, actual) {
                self.emit_with_hint(
                    "override",
                    format!(
                        "Signature of \"{}\" incompatible with supertype \"{}\"",
                        function.name, parent.name
                    ),
                    &self.function_range(function),
                    Some(format!(
                        "     Superclass:\n         {}\n     Subclass:\n{}",
                        self.context.name(expected),
                        format::method(&self.context, method)
                    )),
                );
            }
            break;
        }
    }
}
