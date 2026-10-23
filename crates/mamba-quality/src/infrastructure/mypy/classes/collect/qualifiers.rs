//! Final attributes keep a separate state from properties and frozen data.
use super::{types, Context, Expr};

pub(super) fn is_final(context: &Context, annotation: &Expr, variables: &types::Variables) -> bool {
    let expression = if let Expr::Subscript(subscript) = annotation {
        subscript.value.as_ref()
    } else {
        annotation
    };
    context.typing_form(expression, variables).as_deref() == Some("Final")
}
