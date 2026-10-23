//! Embedded member lookup keeps each original declaration provider's order.
use super::*;

impl Context {
    pub(super) fn embedded_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        self.builtin_buffer_member(ty, name, target)
            .or_else(|| self.string_case_method(ty, name, target))
            .or_else(|| self.aggregation_scalar_method(ty, name, target))
            .or_else(|| self.aggregation_container_method(ty, name, target))
            .or_else(|| self.builtin_conversion_method(ty, name))
            .or_else(|| self.typed_dict_method(ty, name))
            .or_else(|| self.dict_method(ty, name))
            .or_else(|| self.collection_method(ty, name, target))
            .or_else(|| self.nominal_dict_method(ty, name, target))
            .or_else(|| self.builtin_list_item_member(ty, name, target))
    }
}
