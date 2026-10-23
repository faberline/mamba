//! Original signature hints display parameter kinds and optional defaults.
use super::*;

pub(super) fn method(context: &types::Context, method: &Method) -> String {
    method_with_receiver(context, method, false)
}

pub(super) fn method_with_receiver(
    context: &types::Context,
    method: &Method,
    typed_receiver: bool,
) -> String {
    if !method.signature.overloads.is_empty() {
        return method
            .signature
            .overloads
            .iter()
            .map(|signature| {
                let variant = Method {
                    signature: signature.clone(),
                    ..method.clone()
                };
                format!("         @overload\n{}", self::method(context, &variant))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if method.kind == MethodKind::Property {
        return format!(
            "         {}",
            context.name(method.signature.returns.unwrap_or(Type::Any))
        );
    }
    let signature = &method.signature;
    let implicit = usize::from(method.kind != MethodKind::Static);
    let mut arguments = Vec::new();
    if implicit != 0 {
        if let Some(receiver) = signature.parameters.first() {
            arguments.push(if typed_receiver {
                format!(
                    "{}: {}",
                    receiver.name,
                    context.name(receiver.ty.unwrap_or(Type::Any))
                )
            } else {
                receiver.name.clone()
            });
        }
    }
    let parameters = &signature.parameters[implicit.min(signature.parameters.len())..];
    let mut star = false;
    for (index, parameter) in parameters.iter().enumerate() {
        if parameter.kind == Kind::KeywordOnly && !star {
            arguments.push("*".into());
            star = true;
        }
        let prefix = match parameter.kind {
            Kind::VariadicPositional => {
                star = true;
                "*"
            }
            Kind::VariadicKeyword => "**",
            _ => "",
        };
        let ty = context.name(parameter.ty.unwrap_or(Type::Any));
        let mut argument = if parameter.kind == Kind::PositionalOnly {
            ty
        } else {
            format!("{prefix}{}: {ty}", parameter.name)
        };
        if !parameter.required
            && matches!(
                parameter.kind,
                Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::KeywordOnly
            )
        {
            argument.push_str(" = ...");
        }
        arguments.push(argument);
        if parameter.kind == Kind::PositionalOnly
            && parameters
                .get(index + 1)
                .is_none_or(|next| next.kind != Kind::PositionalOnly)
        {
            arguments.push("/".into());
        }
    }
    let decorator = match method.kind {
        MethodKind::Static => "         @staticmethod\n",
        MethodKind::Class => "         @classmethod\n",
        _ => "",
    };
    format!(
        "{decorator}         def {}({}) -> {}",
        signature.name.as_deref().unwrap_or(""),
        arguments.join(", "),
        context.name(signature.returns.unwrap_or(Type::Any))
    )
}
