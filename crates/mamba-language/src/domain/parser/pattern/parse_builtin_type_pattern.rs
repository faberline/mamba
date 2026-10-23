use super::*;

/// Parse a built-in type keyword (`int`, `str`, `float`, etc.) as a class
/// pattern head. If followed by `(`, parse as ClassPattern; otherwise
/// treat as a bare constructor with no fields.
impl<'a> Parser<'a> {
    pub(super) fn parse_builtin_type_pattern(&mut self) -> crate::error::Result<Spanned<Pattern>> {
        let start = self.peek().unwrap().start;
        // Map the token kind to its string name
        let type_name = match self.peek_kind().unwrap() {
            TokenKind::IntType => "int",
            TokenKind::FloatType => "float",
            TokenKind::BoolType => "bool",
            TokenKind::StrType => "str",
            TokenKind::ListType => "list",
            TokenKind::DictType => "dict",
            TokenKind::TupleType => "tuple",
            _ => unreachable!(),
        };
        self.advance(); // consume the type keyword token

        let cls = vec![type_name.to_string()];
        if self.peek_kind() == Some(TokenKind::LParen) {
            self.parse_class_or_constructor_args(cls, start)
        } else {
            // bare type name without parens — Constructor with no fields
            Ok(Spanned::new(
                Pattern::Constructor {
                    path: cls,
                    fields: Vec::new(),
                },
                self.span_from(start),
            ))
        }
    }
}
