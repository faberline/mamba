//! Built-in collection methods retain the pinned declarations and base parameters.
use super::super::{
    classes::{Abstractness, Method, MethodKind},
    declarations::Reader,
    signatures::Signature,
};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

fn data(target: PythonTarget) -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    let key = match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    };
    &DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/stdlib/collection-methods.json"
        ))
        .expect("locked collection methods")
    })["targets"][key]
}

fn reader(context: &Context) -> Reader<'_> {
    Reader {
        context,
        aliases: BTreeMap::new(),
        original_aliases: BTreeMap::new(),
        variables: BTreeMap::new(),
        aliases_ready: true,
    }
}

impl Context {
    fn collection_receiver(&self, ty: Type) -> Option<(&'static str, Vec<Type>)> {
        if let Some(Node::Literal(value) | Node::LiteralValue(value)) = self.node(ty).as_deref() {
            return self.collection_receiver(value.base());
        }
        match ty {
            Type::Str => return Some(("builtins.str", Vec::new())),
            Type::Bytes => return Some(("builtins.bytes", Vec::new())),
            _ => {}
        }
        match self.node(ty)?.as_ref() {
            Node::Container(kind, arguments) => Some((
                match kind {
                    Container::List => "builtins.list",
                    Container::Set => "builtins.set",
                    Container::FrozenSet => "builtins.frozenset",
                    Container::Dict => "builtins.dict",
                    Container::Sequence => "typing.Sequence",
                    Container::Mapping => "typing.Mapping",
                    Container::Iterable => "typing.Iterable",
                    Container::Iterator => "typing.Iterator",
                    Container::Generator => "typing.Generator",
                    Container::DictKeys => "_collections_abc.dict_keys",
                    Container::DictValues => "_collections_abc.dict_values",
                    Container::DictItems => "_collections_abc.dict_items",
                    _ => return None,
                },
                arguments.clone(),
            )),
            Node::Tuple(items) => Some(("builtins.tuple", vec![self.union(items.clone())])),
            Node::VariadicTuple(item) => Some(("builtins.tuple", vec![*item])),
            _ => None,
        }
    }

    fn collection_base_arguments(
        &self,
        reader: &mut Reader<'_>,
        classes: &Value,
        name: &str,
        arguments: &[Type],
        owner: &str,
    ) -> Option<Vec<Type>> {
        if name == owner {
            return Some(arguments.to_vec());
        }
        let class = classes.get(name)?;
        let variables: Vec<_> = class["generic"]
            .as_array()?
            .iter()
            .map(|value| reader.ty(value))
            .collect();
        let solutions = variables
            .into_iter()
            .zip(arguments.iter().copied())
            .collect();
        for base in class["bases"].as_array()? {
            let Some(base_name) = base["type_ref"].as_str().or_else(|| base.as_str()) else {
                continue;
            };
            let arguments: Vec<_> = base["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|value| self.substitute(reader.ty(value), &solutions))
                .collect();
            if let Some(arguments) =
                self.collection_base_arguments(reader, classes, base_name, &arguments, owner)
            {
                return Some(arguments);
            }
        }
        None
    }

    pub(super) fn collection_method(
        &self,
        ty: Type,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let (receiver, arguments) = self.collection_receiver(ty)?;
        let key = (ty, name.to_owned());
        if let Some(signature) = self.classes.borrow().collection_methods.get(&key) {
            return Some(signature.clone());
        }
        let data = data(target);
        let method = data["classes"][receiver]["methods"].get(name)?;
        let owner = method["owner"].as_str()?;
        let mut reader = reader(self);
        let parameters: Vec<_> = data["classes"][owner]["generic"]
            .as_array()?
            .iter()
            .map(|value| reader.ty(value))
            .collect();
        let arguments = self.collection_base_arguments(
            &mut reader,
            &data["classes"],
            receiver,
            &arguments,
            owner,
        )?;
        let solutions = parameters.into_iter().zip(arguments).collect();
        let mut signature = reader.signature(&method["type"]);
        signature = self.substitute_callable(&signature, &solutions).bound(1);
        signature.name = Some(name.into());
        signature.native_symbol = None;
        signature.owner = owner.rsplit('.').next().map(str::to_owned);
        signature.definition_module = Some("builtins".into());
        signature.bound_receiver = Some("self".into());
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_methods
            .insert(key, signature.clone());
        Some(signature)
    }

    pub(in crate::infrastructure::mypy) fn standard_collection_function(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let key = format!("{target:?}:{name}");
        if let Some(signature) = self.classes.borrow().collection_functions.get(&key) {
            return Some(signature.clone());
        }
        let data = data(target);
        let function = data["functions"].get(name)?;
        if name == "len" {
            self.standard_collection("typing", "Sized", target)?;
        }
        let slot = match target {
            PythonTarget::Py312 => 0,
            PythonTarget::Py313 => 1,
            PythonTarget::Py314 => 2,
        };
        let mut reader = reader(self);
        if !self.classes.borrow().collection_helpers[slot] {
            self.classes.borrow_mut().collection_helpers[slot] = true;
            for fullname in [
                "_typeshed.SupportsIter",
                "_typeshed.SupportsNext",
                "builtins._GetItemIterable",
            ] {
                let value = &data["classes"][fullname];
                let short = fullname.rsplit('.').next()?;
                let id = self.declare_class(fullname, short, 0);
                let mut class = (*self.class(id)).clone();
                class.protocol = value["protocol"].as_bool().expect("iteration protocol");
                class.generic = value["generic"]
                    .as_array()
                    .expect("protocol parameters")
                    .iter()
                    .map(|value| reader.ty(value))
                    .collect();
                for (name, value) in value["methods"].as_object().expect("protocol methods") {
                    let mut signature = reader.signature(&value["type"]);
                    signature.name = Some(name.clone());
                    signature.native_symbol = None;
                    signature.owner = Some(short.into());
                    signature.definition_module = Some(fullname.into());
                    class.methods.insert(
                        name.clone(),
                        Method {
                            signature: Arc::new(signature),
                            kind: MethodKind::Instance,
                            setter: None,
                            final_method: false,
                            abstractness: Abstractness::Explicit,
                        },
                    );
                }
                self.replace_class(id, class);
            }
        }
        let mut signature = reader.signature(&function["type"]);
        signature.name = Some(name.into());
        signature.native_symbol = None;
        signature.definition_module = Some("builtins".into());
        for variant in Arc::make_mut(&mut signature.overloads) {
            let variant = Arc::make_mut(variant);
            variant.name = Some(name.into());
            variant.native_symbol = None;
            variant.definition_module = Some("builtins".into());
        }
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .collection_functions
            .insert(key, signature.clone());
        Some(signature)
    }
}
