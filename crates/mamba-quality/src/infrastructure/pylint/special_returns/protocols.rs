//! Fixed result protocols retain distinct original messages and predicates.
pub(super) struct Protocol {
    pub rule: &'static str,
    pub symbol: &'static str,
    pub message: &'static str,
    pub expected: &'static str,
    pub nonnegative: bool,
    pub structured: bool,
}
pub(super) fn lookup(name: &str) -> Option<Protocol> {
    let (rule, symbol, message, expected) = match name {
        "__iter__" => (
            "E0301",
            "non-iterator-returned",
            "__iter__ returns non-iterator",
            "iterator",
        ),
        "__len__" => (
            "E0303",
            "invalid-length-returned",
            "__len__ does not return non-negative integer",
            "int",
        ),
        "__bool__" => (
            "E0304",
            "invalid-bool-returned",
            "__bool__ does not return bool",
            "bool",
        ),
        "__index__" => (
            "E0305",
            "invalid-index-returned",
            "__index__ does not return int",
            "int",
        ),
        "__repr__" => (
            "E0306",
            "invalid-repr-returned",
            "__repr__ does not return str",
            "str",
        ),
        "__str__" => (
            "E0307",
            "invalid-str-returned",
            "__str__ does not return str",
            "str",
        ),
        "__bytes__" => (
            "E0308",
            "invalid-bytes-returned",
            "__bytes__ does not return bytes",
            "bytes",
        ),
        "__hash__" => (
            "E0309",
            "invalid-hash-returned",
            "__hash__ does not return int",
            "int",
        ),
        "__length_hint__" => (
            "E0310",
            "invalid-length-hint-returned",
            "__length_hint__ does not return non-negative integer",
            "int",
        ),
        "__format__" => (
            "E0311",
            "invalid-format-returned",
            "__format__ does not return str",
            "str",
        ),
        "__getnewargs__" => (
            "E0312",
            "invalid-getnewargs-returned",
            "__getnewargs__ does not return a tuple",
            "tuple",
        ),
        "__getnewargs_ex__" => (
            "E0313",
            "invalid-getnewargs-ex-returned",
            "__getnewargs_ex__ does not return a tuple containing (tuple, dict)",
            "tuple",
        ),
        _ => return None,
    };
    Some(Protocol {
        rule,
        symbol,
        message,
        expected,
        nonnegative: matches!(name, "__len__" | "__length_hint__"),
        structured: name == "__getnewargs_ex__",
    })
}
