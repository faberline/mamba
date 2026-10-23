//! Repeated target names retain the original tuple and dummy-name exemptions.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::Expr;
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

impl Checker<'_> {
    pub(super) fn check_repeated_target(&mut self, target: &Expr) {
        let Expr::Tuple(tuple) = target else {
            return;
        };
        let mut names = Vec::new();
        for value in &tuple.elts {
            if matches!(value, Expr::Tuple(_)) {
                self.check_repeated_target(value);
            } else if let Expr::Name(name) = value {
                if name.id.as_str() != "_" {
                    if super::defaults::fixed().ignored(name.id.as_str(), false) {
                        return;
                    }
                    names.push(name.id.to_string());
                }
            }
        }
        let mut counts = BTreeMap::new();
        for (index, name) in names.iter().enumerate() {
            let count = counts.entry(name).or_insert((0, index));
            count.0 += 1;
        }
        let mut counts: Vec<_> = counts
            .into_iter()
            .filter(|(_, (count, _))| *count > 1)
            .collect();
        counts.sort_by_key(|(_, (count, first))| (std::cmp::Reverse(*count), *first));
        for (name, _) in counts {
            self.emit(
                "W0128",
                "redeclared-assigned-name",
                &format!("Redeclared variable '{name}' in assignment"),
                tuple.range(),
            );
        }
    }
}
