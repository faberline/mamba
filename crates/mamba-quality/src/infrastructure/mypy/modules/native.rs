//! Embedded standard module symbols retain their target-version availability.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

mod annotations;
mod names;
mod standard_functions;
mod standard_modules;
mod system;
mod template_strings;
mod typed_children;

impl Lookup {
    pub(in crate::infrastructure::mypy) fn native_pydantic_module(name: &str) -> bool {
        matches!(
            name,
            "pydantic"
                | "pydantic.main"
                | "pydantic.config"
                | "pydantic.fields"
                | "pydantic.json_schema"
                | "pydantic.root_model"
        )
    }

    pub(super) fn native_member(&self, module: &str, member: &str) -> Option<Export> {
        self.native_member_for(module, member, false)
    }

    // Source mode admits only the semantic branches, not ordinary declarations.
    pub(super) fn native_member_for(
        &self,
        module: &str,
        member: &str,
        source: bool,
    ) -> Option<Export> {
        if !source && module == "types" && member == "ModuleType" {
            return self.module_type_member();
        }
        if !source && module == "ast" && member == "Constant" {
            let context = &self.modules.context;
            let target = context.module_target(&self.namespace());
            let instance = context.ast_constant_class(target);
            let ty = context.class_value_type(instance);
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor_for(ty, target),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: Some(instance),
            });
        }
        if let Some(export) = self.name_member(module, member) {
            return Some(export);
        }
        if member == "Buffer"
            && matches!(
                module,
                "typing_extensions" | "collections.abc" | "_collections_abc"
            )
        {
            let old = self.standard_module_member(module, member)?;
            let context = &self.modules.context;
            let target = context.module_target(&self.namespace());
            let instance = context.conversion_protocol("Buffer");
            let ty = context.class_value_type(instance);
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor_for(ty, target),
                module: None,
                explicit: old.explicit,
                module_hidden: old.module_hidden,
                alias: Some(instance),
            });
        }
        if matches!(module, "typing" | "typing_extensions")
            && matches!(
                member,
                "SupportsInt"
                    | "SupportsIndex"
                    | "SupportsFloat"
                    | "SupportsComplex"
                    | "SupportsBytes"
            )
        {
            let context = &self.modules.context;
            let target = context.module_target(&self.namespace());
            let explicit = context.standard_collection_symbol(module, member, target)?;
            let instance = context.conversion_protocol(member);
            let ty = context.class_value_type(instance);
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor_for(ty, target),
                module: None,
                explicit,
                module_hidden: None,
                alias: Some(instance),
            });
        }
        if !source {
            if let Some(export) = self.standard_protocol_member(module, member) {
                return Some(export);
            }
        }
        if !source && module == "string.templatelib" {
            return self.template_member(member);
        }
        if !source && module == "sys" {
            return self.system_member(member);
        }
        if !source
            && matches!(module, "typing" | "collections.abc")
            && matches!(
                member,
                "AbstractSet"
                    | "Set"
                    | "MutableSet"
                    | "Collection"
                    | "Container"
                    | "Sized"
                    | "Iterable"
                    | "Iterator"
                    | "Reversible"
            )
        {
            let context = &self.modules.context;
            if let Some(ty) = context.standard_collection(
                module,
                member,
                context.module_target(&self.namespace()),
            ) {
                return Some(Export {
                    annotation_module: false,
                    ty: Some(ty),
                    callable: context.class_constructor(ty),
                    module: None,
                    explicit: true,
                    module_hidden: Some(false),
                    alias: Some(context.annotation_value(ty)),
                });
            }
        }
        if !source && matches!(module, "typing" | "collections.abc") && member == "MutableMapping" {
            let context = &self.modules.context;
            let target = context.module_target(&self.namespace());
            let instance = context.builtin_value_class_instance("typing.MutableMapping", target)?;
            let ty = context.class_value_type(instance);
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor_for(ty, target),
                module: None,
                explicit: context.standard_collection_symbol(module, member, target)?,
                module_hidden: None,
                alias: Some(instance),
            });
        }
        if !source && matches!(module, "pydantic" | "pydantic.root_model") && member == "RootModel"
        {
            let context = &self.modules.context;
            let ty = context.pydantic_root_model(context.module_target(&self.namespace()));
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor(ty),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.instance_type(ty),
            });
        }
        if matches!(module, "typing" | "collections.abc") {
            let context = &self.modules.context;
            if let Some(explicit) = context.standard_collection_symbol(
                module,
                member,
                context.module_target(&self.namespace()),
            ) {
                if module == "typing" {
                    let fullname = format!("{module}.{member}");
                    if let Some(ty) = context.builtin_value_type_parameter(
                        &fullname,
                        &self.namespace(),
                        context.module_target(&self.namespace()),
                    ) {
                        return Some(Export {
                            annotation_module: false,
                            ty: Some(ty),
                            callable: None,
                            module: None,
                            explicit,
                            module_hidden: None,
                            alias: Some(ty),
                        });
                    }
                }
                // Symbol availability alone does not own the source declaration.
                if source {
                    return None;
                }
                let target = context.module_target(&self.namespace());
                // TypeVar declarations keep their semantic binder, not a class constructor.
                let instance = match (module, member) {
                    ("typing", "TypeVar") => None,
                    _ => {
                        context.builtin_value_class_instance(&format!("{module}.{member}"), target)
                    }
                };
                if let Some(instance) = instance {
                    let ty = context.class_value_type(instance);
                    return Some(Export {
                        annotation_module: false,
                        ty: Some(ty),
                        callable: context.class_constructor_for(ty, target),
                        module: None,
                        explicit,
                        module_hidden: None,
                        alias: Some(instance),
                    });
                }
                return Some(Export {
                    annotation_module: false,
                    ty: Some(Type::Any),
                    callable: None,
                    module: None,
                    explicit,
                    module_hidden: None,
                    alias: if matches!(module, "typing" | "collections.abc") && member == "Callable"
                    {
                        self.standard_module_member(module, member)
                            .and_then(|export| export.alias)
                            .filter(|&alias| context.is_callable_form(alias))
                    } else {
                        None
                    },
                });
            }
        }
        if !source && module == "functools" && member == "cached_property" {
            let context = &self.modules.context;
            let ty = context.cached_property_class(context.module_target(&self.namespace()));
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor(ty),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.instance_type(ty),
            });
        }
        if matches!(module, "pydantic" | "pydantic.fields") && member == "computed_field" {
            let context = &self.modules.context;
            let signature =
                context.pydantic_computed_field(context.module_target(&self.namespace()));
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if !source
            && module == "pydantic.fields"
            && matches!(member, "FieldInfo" | "ComputedFieldInfo")
        {
            let context = &self.modules.context;
            let ty =
                context.pydantic_info_class(member, context.module_target(&self.namespace()))?;
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor(ty),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.instance_type(ty),
            });
        }
        if !source && module == "pydantic.json_schema" && member == "GenerateJsonSchema" {
            let context = &self.modules.context;
            let ty = context.pydantic_schema_generator(context.module_target(&self.namespace()));
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor(ty),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.instance_type(ty),
            });
        }
        if !source
            && ((matches!(module, "pydantic" | "pydantic.main") && member == "BaseModel")
                || (matches!(module, "pydantic" | "pydantic.config") && member == "ConfigDict"))
        {
            let context = &self.modules.context;
            let ty =
                context.pydantic_model_symbol(member, context.module_target(&self.namespace()))?;
            return Some(Export {
                annotation_module: false,
                ty: Some(ty),
                callable: context.class_constructor(ty),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.instance_type(ty),
            });
        }
        if matches!(module, "pydantic" | "pydantic.fields") && member == "Field" {
            let context = &self.modules.context;
            let signature =
                context.pydantic_field_function(context.module_target(&self.namespace()));
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if !source
            && module == "pydantic"
            && !Self::native_pydantic_module(&format!("{module}.{member}"))
        {
            let context = &self.modules.context;
            return Some(Export {
                annotation_module: false,
                ty: Some(context.pydantic_root_getattr(context.module_target(&self.namespace()))),
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if !source && module == "builtins" {
            return self.builtin_member(member);
        }
        if module != "copy" {
            return None;
        }
        let context = &self.modules.context;
        if member == "replace" {
            if context.module_target(&self.namespace()) == PythonTarget::Py312 {
                return None;
            }
            let signature = context.copy_replace_function();
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        if source {
            return None;
        }
        matches!(
            member,
            "copy"
                | "deepcopy"
                | "Error"
                | "error"
                | "PyStringMap"
                | "__all__"
                | "__name__"
                | "__file__"
                | "__doc__"
                | "__package__"
                | "__spec__"
                | "__loader__"
        )
        .then_some(Export {
            annotation_module: false,
            ty: Some(Type::Any),
            callable: None,
            module: None,
            explicit: true,
            module_hidden: Some(false),
            alias: None,
        })
    }
}
