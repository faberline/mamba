//! Fixed Pylint 4.1.2 dunder suggestions and lambda exceptions.

pub(super) fn suggestion(name: &str) -> Option<&'static str> {
    match name {
        "__init__" => Some("Instantiate class directly"),
        "__del__" => Some("Use del keyword"),
        "__repr__" => Some("Use repr built-in function"),
        "__str__" => Some("Use str built-in function"),
        "__bytes__" => Some("Use bytes built-in function"),
        "__format__" => Some("Use format built-in function, format string method, or f-string"),
        "__lt__" => Some("Use < operator"),
        "__le__" => Some("Use <= operator"),
        "__eq__" => Some("Use == operator"),
        "__ne__" => Some("Use != operator"),
        "__gt__" => Some("Use > operator"),
        "__ge__" => Some("Use >= operator"),
        "__hash__" => Some("Use hash built-in function"),
        "__bool__" => Some("Use bool built-in function"),
        "__getattr__" => Some("Access attribute directly or use getattr built-in function"),
        "__getattribute__" => Some("Access attribute directly or use getattr built-in function"),
        "__setattr__" => Some("Set attribute directly or use setattr built-in function"),
        "__delattr__" => Some("Use del keyword"),
        "__dir__" => Some("Use dir built-in function"),
        "__get__" => Some("Use get method"),
        "__set__" => Some("Use set method"),
        "__delete__" => Some("Use del keyword"),
        "__instancecheck__" => Some("Use isinstance built-in function"),
        "__subclasscheck__" => Some("Use issubclass built-in function"),
        "__call__" => Some("Invoke instance directly"),
        "__len__" => Some("Use len built-in function"),
        "__length_hint__" => Some("Use length_hint method"),
        "__getitem__" => Some("Access item via subscript"),
        "__setitem__" => Some("Set item via subscript"),
        "__delitem__" => Some("Use del keyword"),
        "__iter__" => Some("Use iter built-in function"),
        "__next__" => Some("Use next built-in function"),
        "__reversed__" => Some("Use reversed built-in function"),
        "__contains__" => Some("Use in keyword"),
        "__add__" => Some("Use + operator"),
        "__sub__" => Some("Use - operator"),
        "__mul__" => Some("Use * operator"),
        "__matmul__" => Some("Use @ operator"),
        "__truediv__" => Some("Use / operator"),
        "__floordiv__" => Some("Use // operator"),
        "__mod__" => Some("Use % operator"),
        "__divmod__" => Some("Use divmod built-in function"),
        "__pow__" => Some("Use ** operator or pow built-in function"),
        "__lshift__" => Some("Use << operator"),
        "__rshift__" => Some("Use >> operator"),
        "__and__" => Some("Use & operator"),
        "__xor__" => Some("Use ^ operator"),
        "__or__" => Some("Use | operator"),
        "__radd__" => Some("Use + operator"),
        "__rsub__" => Some("Use - operator"),
        "__rmul__" => Some("Use * operator"),
        "__rmatmul__" => Some("Use @ operator"),
        "__rtruediv__" => Some("Use / operator"),
        "__rfloordiv__" => Some("Use // operator"),
        "__rmod__" => Some("Use % operator"),
        "__rdivmod__" => Some("Use divmod built-in function"),
        "__rpow__" => Some("Use ** operator or pow built-in function"),
        "__rlshift__" => Some("Use << operator"),
        "__rrshift__" => Some("Use >> operator"),
        "__rand__" => Some("Use & operator"),
        "__rxor__" => Some("Use ^ operator"),
        "__ror__" => Some("Use | operator"),
        "__iadd__" => Some("Use += operator"),
        "__isub__" => Some("Use -= operator"),
        "__imul__" => Some("Use *= operator"),
        "__imatmul__" => Some("Use @= operator"),
        "__itruediv__" => Some("Use /= operator"),
        "__ifloordiv__" => Some("Use //= operator"),
        "__imod__" => Some("Use %= operator"),
        "__ipow__" => Some("Use **= operator"),
        "__ilshift__" => Some("Use <<= operator"),
        "__irshift__" => Some("Use >>= operator"),
        "__iand__" => Some("Use &= operator"),
        "__ixor__" => Some("Use ^= operator"),
        "__ior__" => Some("Use |= operator"),
        "__neg__" => Some("Multiply by -1 instead"),
        "__pos__" => Some("Multiply by +1 instead"),
        "__abs__" => Some("Use abs built-in function"),
        "__invert__" => Some("Use ~ operator"),
        "__complex__" => Some("Use complex built-in function"),
        "__int__" => Some("Use int built-in function"),
        "__float__" => Some("Use float built-in function"),
        "__round__" => Some("Use round built-in function"),
        "__trunc__" => Some("Use math.trunc function"),
        "__floor__" => Some("Use math.floor function"),
        "__ceil__" => Some("Use math.ceil function"),
        "__enter__" => Some("Invoke context manager directly"),
        "__aenter__" => Some("Invoke context manager directly"),
        "__copy__" => Some("Use copy.copy function"),
        "__deepcopy__" => Some("Use copy.deepcopy function"),
        "__fspath__" => Some("Use os.fspath function instead"),
        "__aiter__" => Some("Use aiter built-in function"),
        "__anext__" => Some("Use anext built-in function"),
        _ => None,
    }
}

pub(super) fn lambda_exception(name: &str) -> bool {
    matches!(
        name,
        "__init__"
            | "__del__"
            | "__delattr__"
            | "__set__"
            | "__delete__"
            | "__setitem__"
            | "__delitem__"
            | "__iadd__"
            | "__isub__"
            | "__imul__"
            | "__imatmul__"
            | "__itruediv__"
            | "__ifloordiv__"
            | "__imod__"
            | "__ipow__"
            | "__ilshift__"
            | "__irshift__"
            | "__iand__"
            | "__ixor__"
            | "__ior__"
    )
}
