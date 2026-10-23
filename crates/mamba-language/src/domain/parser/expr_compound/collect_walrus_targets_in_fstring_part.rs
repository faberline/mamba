use super::*;

impl<'a> Parser<'a> {
    pub(super) fn collect_walrus_targets_in_fstring_part(
        part: &FStringPart,
        out: &mut Vec<String>,
    ) {
        match part {
            FStringPart::Literal(_) => {}
            FStringPart::Expr(expr, spec) => {
                Self::collect_walrus_targets(expr, out);
                if let Some(spec) = spec {
                    for part in spec {
                        Self::collect_walrus_targets_in_fstring_part(part, out);
                    }
                }
            }
        }
    }
}
