//! Locked standard module exports keep missing attributes distinct from dynamic values.
use super::{Export, Lookup, Type};
use mamba_language::application::quality_syntax::PythonTarget;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

type Modules = BTreeMap<String, Module>;
type Targets = BTreeMap<String, Modules>;

#[derive(Deserialize)]
struct Data {
    platforms: BTreeMap<String, Targets>,
}
#[derive(Deserialize)]
struct Module {
    dynamic: bool,
    symbols: BTreeMap<String, Symbol>,
}
#[derive(Deserialize)]
struct Symbol {
    explicit: bool,
    module: Option<String>,
}

impl Lookup {
    fn standard_module(&self, module: &str) -> Option<&'static Module> {
        static DATA: OnceLock<Data> = OnceLock::new();
        let data = DATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../../assets/stdlib/standard-module-symbols.json"
            ))
            .expect("locked standard module symbols")
        });
        let platform = if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };
        let target = match self.modules.context.module_target(&self.namespace()) {
            PythonTarget::Py312 => "3.12",
            PythonTarget::Py313 => "3.13",
            PythonTarget::Py314 => "3.14",
        };
        data.platforms.get(platform)?.get(target)?.get(module)
    }

    // A real standard symbol supplies this class, never a dynamic missing export.
    pub(super) fn module_type_member(&self) -> Option<Export> {
        let symbol = self.standard_module("types")?.symbols.get("ModuleType")?;
        if symbol.module.is_some() {
            return None;
        }
        let context = &self.modules.context;
        let target = context.module_target(&self.namespace());
        let instance = context.builtin_value_class_instance("types.ModuleType", target)?;
        let ty = context.class_value_type(instance);
        let callable = context.class_constructor_for(ty, target)?;
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: Some(callable),
            module: None,
            explicit: symbol.explicit,
            module_hidden: None,
            alias: Some(instance),
        })
    }

    pub(in crate::infrastructure::mypy) fn standard_module_exists(&self, module: &str) -> bool {
        self.standard_module(module).is_some()
    }

    pub(in crate::infrastructure::mypy) fn standard_module_member(
        &self,
        module: &str,
        member: &str,
    ) -> Option<Export> {
        let canonical = matches!(
            module,
            "typing" | "typing_extensions" | "collections.abc" | "_collections_abc"
        ) && member == "Callable";
        let module = self.standard_module(module)?;
        let symbol = module.symbols.get(member);
        if symbol.is_none() && !module.dynamic {
            return None;
        }
        let imported_module = symbol.and_then(|symbol| symbol.module.clone());
        let canonical = canonical
            && symbol.is_some_and(|symbol| symbol.explicit && symbol.module.is_none())
            && imported_module.is_none();
        let annotation_module = symbol.is_some() && imported_module.is_some();
        Some(Export {
            annotation_module,
            ty: imported_module.is_none().then_some(Type::Any),
            callable: None,
            module: imported_module,
            explicit: symbol.is_none_or(|symbol| symbol.explicit),
            module_hidden: None,
            alias: canonical.then(|| {
                self.modules
                    .context
                    .intern(crate::infrastructure::mypy::types::Node::TypingForm(
                        "Callable".into(),
                    ))
            }),
        })
    }
}
