use super::*;

pub(super) fn emit_after(out: &mut Vec<Spanned<Stmt>>, item: AfterItem) {
    out.extend(item.tv_assigns);
    for (attr, tps) in &item.attrs {
        out.push(attr_tuple_assign(&item.path, attr, tps, item.span));
    }
    for (attr, value) in &item.extra_attrs {
        out.push(attr_assign(&item.path, attr, value.clone(), item.span));
    }
}
