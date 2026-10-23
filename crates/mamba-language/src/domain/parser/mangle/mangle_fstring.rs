use super::*;

pub(super) fn mangle_fstring(parts: &mut [FStringPart], class: Option<&str>) {
    for part in parts.iter_mut() {
        if let FStringPart::Expr(e, spec) = part {
            mangle_expr(e, class);
            if let Some(sp) = spec {
                mangle_fstring(sp, class);
            }
        }
    }
}
