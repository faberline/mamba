//! Legacy nongraph annotation imports retain their native class aliases.
use super::*;

impl Lookup {
    pub(in crate::infrastructure::mypy) fn native_class_annotations(
        &self,
        module: &str,
    ) -> Vec<(String, Type)> {
        if module == "builtins" {
            let names = [
                "int",
                "float",
                "complex",
                "str",
                "bytes",
                "bool",
                "object",
                "list",
                "dict",
                "set",
                "frozenset",
                "tuple",
                "reversed",
                "enumerate",
                "range",
                "zip",
            ];
            let target = self.modules.context.module_target(&self.namespace());
            return names
                .into_iter()
                .map(String::from)
                .chain(Context::builtin_value_class_names(target))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .filter_map(|name| Some((name.clone(), self.builtin_member(&name)?.alias?)))
                .collect();
        }
        let members: &[(&str, &str, &str)] = match module {
            "ast" => &[("Constant", "ast", "Constant")],
            "typing" => &[
                ("SupportsInt", "typing", "SupportsInt"),
                ("SupportsIndex", "typing", "SupportsIndex"),
                ("SupportsFloat", "typing", "SupportsFloat"),
                ("SupportsComplex", "typing", "SupportsComplex"),
                ("SupportsBytes", "typing", "SupportsBytes"),
                ("AbstractSet", "typing", "AbstractSet"),
                ("MutableMapping", "typing", "MutableMapping"),
                ("MutableSet", "typing", "MutableSet"),
                ("Collection", "typing", "Collection"),
                ("Container", "typing", "Container"),
                ("Sized", "typing", "Sized"),
                ("Iterable", "typing", "Iterable"),
                ("Iterator", "typing", "Iterator"),
                ("Reversible", "typing", "Reversible"),
            ],
            "typing_extensions" => &[
                ("Buffer", "typing_extensions", "Buffer"),
                ("SupportsInt", "typing_extensions", "SupportsInt"),
                ("SupportsIndex", "typing_extensions", "SupportsIndex"),
                ("SupportsFloat", "typing_extensions", "SupportsFloat"),
                ("SupportsComplex", "typing_extensions", "SupportsComplex"),
                ("SupportsBytes", "typing_extensions", "SupportsBytes"),
            ],
            "_collections_abc" => &[("Buffer", "_collections_abc", "Buffer")],
            "collections.abc" => &[
                ("Buffer", "collections.abc", "Buffer"),
                ("Set", "collections.abc", "Set"),
                ("MutableMapping", "collections.abc", "MutableMapping"),
                ("MutableSet", "collections.abc", "MutableSet"),
                ("Collection", "collections.abc", "Collection"),
                ("Container", "collections.abc", "Container"),
                ("Sized", "collections.abc", "Sized"),
                ("Iterable", "collections.abc", "Iterable"),
                ("Iterator", "collections.abc", "Iterator"),
                ("Reversible", "collections.abc", "Reversible"),
            ],
            "collections" => &[
                ("abc.Buffer", "collections.abc", "Buffer"),
                ("abc.Set", "collections.abc", "Set"),
                ("abc.MutableMapping", "collections.abc", "MutableMapping"),
                ("abc.MutableSet", "collections.abc", "MutableSet"),
                ("abc.Collection", "collections.abc", "Collection"),
                ("abc.Container", "collections.abc", "Container"),
                ("abc.Sized", "collections.abc", "Sized"),
                ("abc.Iterable", "collections.abc", "Iterable"),
                ("abc.Iterator", "collections.abc", "Iterator"),
                ("abc.Reversible", "collections.abc", "Reversible"),
            ],
            "pydantic" => &[
                ("BaseModel", "pydantic", "BaseModel"),
                ("RootModel", "pydantic", "RootModel"),
                ("root_model.RootModel", "pydantic.root_model", "RootModel"),
                ("ConfigDict", "pydantic", "ConfigDict"),
                ("main.BaseModel", "pydantic.main", "BaseModel"),
                ("config.ConfigDict", "pydantic.config", "ConfigDict"),
                ("fields.FieldInfo", "pydantic.fields", "FieldInfo"),
                (
                    "fields.ComputedFieldInfo",
                    "pydantic.fields",
                    "ComputedFieldInfo",
                ),
            ],
            "pydantic.main" => &[("BaseModel", "pydantic.main", "BaseModel")],
            "pydantic.root_model" => &[("RootModel", "pydantic.root_model", "RootModel")],
            "pydantic.config" => &[("ConfigDict", "pydantic.config", "ConfigDict")],
            "_typeshed" => &[
                ("SupportsRead", "_typeshed", "SupportsRead"),
                ("SupportsWrite", "_typeshed", "SupportsWrite"),
            ],
            "functools" => &[("cached_property", "functools", "cached_property")],
            "string.templatelib" => &[
                ("Template", "string.templatelib", "Template"),
                ("Interpolation", "string.templatelib", "Interpolation"),
            ],
            "pydantic.fields" => &[
                ("FieldInfo", "pydantic.fields", "FieldInfo"),
                ("ComputedFieldInfo", "pydantic.fields", "ComputedFieldInfo"),
            ],
            "pydantic.json_schema" => &[(
                "GenerateJsonSchema",
                "pydantic.json_schema",
                "GenerateJsonSchema",
            )],
            _ => &[],
        };
        members
            .iter()
            .filter_map(|&(local, module, member)| {
                Some((local.into(), self.native_member(module, member)?.alias?))
            })
            .collect()
    }
}
