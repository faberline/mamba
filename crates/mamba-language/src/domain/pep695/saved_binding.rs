use super::*;

pub(super) fn saved_binding(name: &str, slot: usize, span: Span) -> SavedBinding {
    SavedBinding {
        name: name.to_string(),
        saved_name: format!("__mb_pep695_saved_{}_{slot}", span.start),
        had_name: format!("__mb_pep695_had_{}_{slot}", span.start),
    }
}
