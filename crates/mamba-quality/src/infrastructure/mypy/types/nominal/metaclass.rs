//! Class objects also read members from their declared metaclass instances.
use super::*;

impl Context {
    pub(super) fn class_object_target(&self, ty: Type) -> PythonTarget {
        self.class_id(ty)
            .map(|id| self.module_target(&self.class(id).namespace))
            .unwrap_or(PythonTarget::Py312)
    }

    pub(in crate::infrastructure::mypy) fn member_is_instance(&self, ty: Type, name: &str) -> bool {
        !self.is_class_value(ty)
            || self
                .metaclass_member_receiver(ty, name, self.class_object_target(ty))
                .is_some()
    }

    pub(super) fn metaclass_member_receiver(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        if !self.is_class_value(ty) {
            return None;
        }
        let id = self.class_id(ty)?;
        if self
            .class_lineage(id)
            .into_iter()
            .any(|id| self.class(id).erased_members.contains(name))
        {
            return None;
        }
        let builtin = self.runtime_builtin_class_instance("type", target);
        let meta = self
            .recorded_metaclass(id)
            .map(|id| self.class(id).instance)
            .or(builtin)?;
        let Some(owner) = self.nominal_member_owner(ty, name) else {
            return Some(meta);
        };
        let class = self.class(owner);
        // Metaclass members take priority over a non-ClassVar variable.
        // Represented methods, decorated members and ClassVars stay intact.
        class
            .fields
            .get(name)
            .filter(|field| !field.class_var && !class.decorated_fields.contains(name))?;
        self.nominal_member_owner(meta, name).map(|_| meta)
    }

    pub(super) fn metaclass_field(&self, receiver: Type, meta: Type, name: &str) -> Option<Field> {
        let mut field = self.raw_class_field(meta, name)?;
        // The class object is an instance of meta, so instance members apply.
        field.class_access = true;
        if let Some(descriptor) = self.descriptor_member(meta, name) {
            if let Some(result) = self.metaclass_descriptor_read_type(descriptor, receiver, meta) {
                field.ty = Some(result);
            }
        }
        Some(field)
    }

    fn metaclass_descriptor_read_type(
        &self,
        descriptor: Type,
        receiver: Type,
        meta: Type,
    ) -> Option<Type> {
        if let Some(Node::Union(items)) = self.node(descriptor).as_deref() {
            return Some(
                self.union(
                    items
                        .iter()
                        .map(|&item| {
                            self.metaclass_descriptor_read_type(item, receiver, meta)
                                .unwrap_or(item)
                        })
                        .collect(),
                ),
            );
        }
        let signature = self.class_method(descriptor, "__get__")?;
        // A metaclass descriptor sees the actual class object, not None.
        let arguments = [receiver, self.class_value_type(meta)];
        let signature = if signature.overloads.is_empty() {
            signature
        } else {
            signature
                .overloads
                .iter()
                .find(|variant| {
                    variant
                        .parameters
                        .iter()
                        .zip(arguments)
                        .all(|(parameter, actual)| {
                            parameter
                                .ty
                                .is_none_or(|expected| self.accepts(expected, actual))
                        })
                })
                .cloned()
                .or_else(|| signature.overloads.first().cloned())?
        };
        let positional = signature
            .parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    super::super::super::signatures::Kind::PositionalOnly
                        | super::super::super::signatures::Kind::PositionalOrKeyword
                )
            })
            .collect::<Vec<_>>();
        let variadic = signature.parameters.iter().find(|parameter| {
            parameter.kind == super::super::super::signatures::Kind::VariadicPositional
        });
        let pairs = arguments
            .into_iter()
            .enumerate()
            .filter_map(|(index, actual)| {
                Some((positional.get(index).copied().or(variadic)?.ty?, actual))
            })
            .collect::<Vec<_>>();
        let solution = self.solve_call(&signature.generic, &pairs, None);
        signature
            .returns
            .map(|returns| self.substitute_inferred_alias(returns, &solution.types))
    }

    pub(in crate::infrastructure::mypy) fn class_member_owner(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<u32> {
        let receiver = self
            .metaclass_member_receiver(ty, name, self.class_object_target(ty))
            .unwrap_or(ty);
        self.nominal_member_owner(receiver, name)
    }

    fn nominal_member_owner(&self, ty: Type, name: &str) -> Option<u32> {
        self.class_lineage(self.class_id(ty)?)
            .into_iter()
            .find(|&id| {
                self.class(id).fields.contains_key(name)
                    || self.class(id).methods.contains_key(name)
            })
    }
    pub(in crate::infrastructure::mypy) fn class_write_type(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Type> {
        if let Some(meta) = self.metaclass_member_receiver(ty, name, self.class_object_target(ty)) {
            return self.class_write_type(meta, name);
        }
        for id in self.class_lineage(self.class_id(ty)?) {
            let class = self.class(id);
            if let Some(method) = class.methods.get(name) {
                if method.kind == MethodKind::Property {
                    return method
                        .setter
                        .as_ref()
                        .and_then(|signature| signature.parameters.get(1))
                        .and_then(|parameter| parameter.ty)
                        .or(method.signature.returns)
                        .map(|member| self.substitute(member, &self.class_solutions(ty, id)));
                }
            }
            if let Some(field) = class.fields.get(name) {
                if let Some(descriptor) = self.descriptor_member(ty, name) {
                    if !self.is_class_value(ty) {
                        if let Some(value) = self.descriptor_write_type(descriptor, ty) {
                            return Some(value);
                        }
                    }
                }
                return field
                    .ty
                    .map(|member| self.substitute(member, &self.class_solutions(ty, id)));
            }
        }
        None
    }
}
