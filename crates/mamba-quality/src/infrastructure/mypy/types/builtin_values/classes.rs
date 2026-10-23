//! Locked builtin and referenced classes share the native class graph.
use super::*;
use crate::infrastructure::mypy::classes::{Abstractness, Field, Method, MethodKind};

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_value_class_instance(
        &self,
        fullname: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        data(target)["classes"].get(fullname)?;
        self.load_builtin_value_classes(target);
        let id = self
            .classes
            .borrow()
            .keys
            .get(&(fullname.into(), 0))
            .copied()?;
        Some(self.class(id).instance)
    }

    pub(in crate::infrastructure::mypy) fn builtin_value_type_parameter(
        &self,
        fullname: &str,
        namespace: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        let (project, _) = namespace.split_once(':')?;
        let declarations = data(target)["classes"].as_object()?;
        let mut parameter = declarations
            .values()
            .flat_map(|class| class["generic"].as_array().into_iter().flatten())
            .find(|parameter| {
                parameter[".class"] == "TypeVarType"
                    && parameter["fullname"].as_str() == Some(fullname)
            })?
            .clone();
        // The imported symbol has its own stable binder, not its template owner's.
        parameter["namespace"] = format!("<builtin-import:{target:?}>:{project}:{fullname}").into();
        self.load_builtin_value_classes(target);
        let mut reader = self.builtin_value_reader(target);
        let ty = reader.ty(&parameter);
        self.classes.borrow_mut().builtin_value_variables[slot(target)] = reader.variables.into();
        Some(ty)
    }

    pub(super) fn load_builtin_value_classes(&self, target: PythonTarget) {
        let slot = slot(target);
        if self.classes.borrow().builtin_value_libraries[slot] {
            return;
        }
        let data = data(target);
        let declarations = data["classes"].as_object().expect("builtin classes");
        let ids: BTreeMap<_, _> = declarations
            .keys()
            .map(|name| {
                let id = name
                    .strip_prefix("builtins.")
                    .and_then(Context::builtin_name)
                    .map(|ty| self.builtin_class(ty))
                    .unwrap_or_else(|| {
                        self.declare_class(name, name.rsplit('.').next().unwrap(), 0)
                    });
                (name.clone(), id)
            })
            .collect();
        let aliases = data["aliases"].as_object().expect("builtin aliases");
        let mut reader = Reader {
            context: self,
            variables: BTreeMap::new(),
            aliases_ready: false,
            original_aliases: aliases
                .iter()
                .map(|(name, value)| (name.clone(), value["target"].clone()))
                .collect(),
            aliases: aliases
                .keys()
                .enumerate()
                .map(|(index, name)| {
                    let id = self.declare_alias(
                        &format!("<builtin-values:{slot}>:{name}"),
                        name.rsplit('.').next().unwrap(),
                        u32::try_from(index).unwrap(),
                        false,
                        false,
                    );
                    (name.clone(), id)
                })
                .collect(),
        };
        for (name, value) in aliases {
            let parameters = value["variables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| reader.ty(value))
                .collect();
            self.set_embedded_alias_parameters(reader.aliases[name], parameters);
            let ty = reader.ty(&value["target"]);
            self.set_embedded_alias_target(reader.aliases[name], ty);
        }
        reader.aliases_ready = true;
        for (name, value) in declarations {
            let id = ids[name];
            let mut class = (*self.class(id)).clone();
            if class.generic.is_empty() {
                class.generic = value["generic"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| reader.ty(value))
                    .collect();
            }
            if class.base_types.is_empty() && class.scalar_bases.is_empty() {
                let bases: Vec<_> = value["bases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| reader.base_type(value))
                    .map(|ty| {
                        if ty == Type::Object {
                            self.class(self.builtin_class(ty)).instance
                        } else {
                            ty
                        }
                    })
                    .collect();
                class.base_types = bases
                    .iter()
                    .copied()
                    .filter(|&ty| self.class_id(ty).is_some())
                    .collect();
                class.bases = class
                    .base_types
                    .iter()
                    .filter_map(|&ty| self.class_id(ty))
                    .collect();
                class.scalar_bases = bases
                    .into_iter()
                    .filter(|&ty| self.class_id(ty).is_none())
                    .collect();
            }
            class.final_class |= value["final"].as_bool().unwrap();
            class.protocol |= value["protocol"].as_bool().unwrap();
            class.runtime_protocol |= value["runtime_protocol"].as_bool().unwrap();
            for (field, item) in value["fields"].as_object().unwrap() {
                class.fields.entry(field.clone()).or_insert_with(|| Field {
                    ty: Some(reader.ty(&item["type"])),
                    needs_annotation: false,
                    class_access: item["initialized_in_class"].as_bool().unwrap(),
                    readonly: false,
                    final_attribute: item["final"].as_bool().unwrap(),
                    class_var: item["class_var"].as_bool().unwrap(),
                    abstract_field: false,
                });
            }
            for (member, item) in value["methods"].as_object().unwrap() {
                if class.methods.contains_key(member) {
                    continue;
                }
                let mut signature = signatures::read(
                    &mut reader,
                    &item["type"],
                    name.rsplit_once('.').unwrap().0,
                    Some(&class.name),
                );
                signature.name = Some(member.clone());
                let kind = if item["property"] == true {
                    MethodKind::Property
                } else if item["class_method"] == true {
                    MethodKind::Class
                } else if item["static_method"] == true {
                    MethodKind::Static
                } else {
                    MethodKind::Instance
                };
                class.methods.insert(
                    member.clone(),
                    Method {
                        signature: Arc::new(signature),
                        kind,
                        setter: None,
                        final_method: item["final"].as_bool().unwrap(),
                        abstractness: match item["abstract_status"].as_u64().unwrap() {
                            0 => Abstractness::Concrete,
                            1 => Abstractness::Explicit,
                            2 => Abstractness::Implicit,
                            code => panic!("unsupported builtin abstract status: {code}"),
                        },
                    },
                );
            }
            self.replace_class(id, class);
        }
        let mut registry = self.classes.borrow_mut();
        registry.builtin_value_libraries[slot] = true;
        registry.builtin_value_aliases[slot] = reader.aliases;
        registry.builtin_value_variables[slot] = reader.variables.into();
    }
}
