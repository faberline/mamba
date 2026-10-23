use super::*;

// #1582: `lambda x: x, R(...)` inside a call-arg list must parse with
// body=`x` and the comma terminating the surrounding call-arg list — NOT
// with `x` (the body) confused for a type annotation.
#[test]
pub(super) fn test_lambda_in_call_arg_list_terminated_by_comma() {
    let module = parse(
        "def chain(*args): return None\n\
         def map(fn, x): return None\n\
         def R(x): return x\n\
         result = chain(map(lambda x:x, R([1,2,3])))\n",
    );
    assert_eq!(module.stmts.len(), 4);
}
