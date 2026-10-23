#![cfg(test)]

use crate::parser;
use crate::source::span::FileId;
use mamba_shared_kernel::ast::*;

mod case_aug_assign_bare_tuple_rhs;
mod case_aug_assign_trailing_comma_tuple_rhs;
mod case_binary_operators;
mod case_case_as_attribute_name;
mod case_case_as_expression_ident;
mod case_chained_assign_in_class_body_does_not_leak;
mod case_chained_tuple_unpack_assign;
mod case_class_def;
mod case_class_def_metaclass_only;
mod case_class_def_with_metaclass_keyword;
mod case_enum_def;
mod case_for_doubly_nested_paren_target;
mod case_for_loop;
mod case_for_nested_paren_target;
mod case_for_paren_single_target_with_trailing_comma;
mod case_for_paren_tuple_target;
mod case_from_import;
mod case_fstring_simple_expression;
mod case_fstring_with_literal_and_expr;
mod case_function_call;
mod case_function_def;
mod case_generic_type;
mod case_if_elif_else;
mod case_import;
mod case_lambda_in_call_arg_list_terminated_by_comma;
mod case_list_literal;
mod case_match_stmt;
mod case_module_chained_assignment_desugaring_and_following_stmt;
mod case_optional_type;
mod case_single_line_suite_chained_assignment;
mod case_single_line_suite_chained_assignment_with_semicolon;
mod case_subscript_with_comma_tuple_index;
mod case_subscript_with_three_comma_indices;
mod case_type_params;
mod case_unary_neg;
mod case_var_decl;
mod case_while_loop;
mod case_with_paren_tuple_target;
mod parse;

use parse::parse;
