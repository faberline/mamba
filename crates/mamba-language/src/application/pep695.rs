use mamba_shared_kernel::ast::Module;

pub const TYPEVAR_INTRINSIC: &str = "__mb_pep695_typevar__";
pub const TYPE_ALIAS_INTRINSIC: &str = "__mb_pep695_type_alias__";

pub fn desugar_module(module: &mut Module) {
    crate::domain::pep695::desugar_module(module);
}
