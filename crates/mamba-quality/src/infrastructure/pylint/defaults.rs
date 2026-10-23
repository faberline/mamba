//! Fixed original data is embedded and cannot be changed by project settings.
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Deserialize)]
struct Data {
    dummy_variables_rgx: String,
    ignored_argument_names: String,
    allowed_redefined_builtins: BTreeSet<String>,
    redefining_builtins_modules: BTreeSet<String>,
    module_scope_attrs: BTreeSet<String>,
    callbacks: Vec<String>,
    mutable_default_symbols: BTreeMap<String, String>,
    contextmanager_decorators: BTreeSet<String>,
    good_names: BTreeSet<String>,
    bad_names: BTreeSet<String>,
    naming_regexps: BTreeMap<String, String>,
    min_public_methods: usize,
    max_public_methods: usize,
    valid_classmethod_first_arg: Vec<String>,
    valid_metaclass_classmethod_first_arg: Vec<String>,
    invalid_base_classes: BTreeSet<String>,
    max_parents: usize,
    max_attributes: usize,
    ignored_parents: BTreeSet<String>,
    stdlib_classes_ignore_ancestor: BTreeSet<String>,
    defining_attr_methods: BTreeSet<String>,
    dataclasses_decorators: BTreeSet<String>,
    attrs_decorators: BTreeSet<String>,
    mixin_class_rgx: String,
    ignored_checks_for_mixins: BTreeSet<String>,
    special_method_parameters: BTreeMap<String, Option<SpecialParameters>>,
    exclude_protected: BTreeSet<String>,
    check_protected_access_in_special_methods: bool,
    max_args: usize,
    max_positional_arguments: usize,
    max_locals: usize,
    max_returns: usize,
    max_branches: usize,
    max_statements: usize,
    max_bool_expr: usize,
    max_module_lines: usize,
    indent_string: String,
    notes: Vec<String>,
    import_order_standard_library: BTreeSet<String>,
    import_order_known_third_party: BTreeSet<String>,
    deprecated_modules: BTreeSet<String>,
    deprecated_module_versions: BTreeMap<String, BTreeSet<String>>,
    allow_reexport_from_package: bool,
    timeout_methods: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum SpecialParameters {
    Exact(usize),
    Range([usize; 2]),
}

pub(super) struct Defaults {
    dummy: regex::Regex,
    arguments: regex::Regex,
    data: Data,
    naming: BTreeMap<String, regex::Regex>,
    mixin: regex::Regex,
}
pub(super) struct FunctionLimits {
    pub arguments: usize,
    pub positional: usize,
    pub locals: usize,
    pub returns: usize,
    pub branches: usize,
    pub statements: usize,
    pub boolean: usize,
}

pub(super) fn fixed() -> &'static Defaults {
    static DATA: OnceLock<Defaults> = OnceLock::new();
    DATA.get_or_init(|| {
        let data: Data = serde_json::from_str(include_str!("../../../assets/pylint-defaults.json"))
            .expect("locked Pylint default data");
        Defaults {
            mixin: regex::Regex::new(&format!("^(?:{})", data.mixin_class_rgx))
                .expect("Pylint mixin regex"),
            dummy: regex::Regex::new(&format!("^(?:{})", data.dummy_variables_rgx))
                .expect("Pylint dummy regex"),
            arguments: regex::Regex::new(&format!("^(?:{})", data.ignored_argument_names))
                .expect("Pylint argument regex"),
            naming: data
                .naming_regexps
                .iter()
                .filter(|(kind, _)| {
                    matches!(
                        kind.as_str(),
                        "const"
                            | "argument"
                            | "variable"
                            | "class"
                            | "method"
                            | "function"
                            | "attr"
                    )
                })
                .map(|(kind, expression)| {
                    (
                        kind.clone(),
                        regex::Regex::new(&format!("^(?:{expression})"))
                            .expect("Pylint naming regex"),
                    )
                })
                .collect(),
            data,
        }
    })
}

impl Defaults {
    pub(super) fn timeout_method(&self, name: &str) -> bool {
        self.data.timeout_methods.contains(name)
    }
    pub(super) fn import_standard(&self, name: &str) -> bool {
        self.data.import_order_standard_library.contains(name)
    }
    pub(super) fn import_third_party(&self, name: &str) -> bool {
        self.data.import_order_known_third_party.contains(name)
    }
    pub(super) fn allow_package_reexport(&self) -> bool {
        self.data.allow_reexport_from_package
    }
    pub(super) fn deprecated_module(&self, name: &str, minor: u8) -> bool {
        self.data
            .deprecated_modules
            .iter()
            .chain(
                self.data
                    .deprecated_module_versions
                    .iter()
                    .filter(|(version, _)| {
                        let parts: Vec<u8> = version
                            .split('.')
                            .filter_map(|part| part.parse().ok())
                            .collect();
                        parts.as_slice() <= [3, minor, 0].as_slice()
                    })
                    .flat_map(|(_, values)| values),
            )
            .any(|module| name == module || name.starts_with(&format!("{module}.")))
    }
    pub(super) fn bad(&self, name: &str) -> bool {
        self.data.bad_names.contains(name) && !self.good(name)
    }
    pub(super) fn module_lines(&self) -> usize {
        self.data.max_module_lines
    }
    pub(super) fn indent(&self) -> &str {
        &self.data.indent_string
    }
    pub(super) fn notes(&self) -> &[String] {
        &self.data.notes
    }
    pub(super) fn contextmanager(&self, name: &str) -> bool {
        self.data.contextmanager_decorators.contains(name)
    }
    pub(super) fn metaclass_receiver(&self, name: &str) -> bool {
        self.data
            .valid_metaclass_classmethod_first_arg
            .iter()
            .any(|value| value == name)
    }
    pub(super) fn metaclass_receiver_names(&self) -> String {
        self.data
            .valid_metaclass_classmethod_first_arg
            .iter()
            .map(|value| format!("'{value}'"))
            .collect::<Vec<_>>()
            .join(" or ")
    }
    pub(super) fn function_limits(&self) -> FunctionLimits {
        FunctionLimits {
            arguments: self.data.max_args,
            positional: self.data.max_positional_arguments,
            locals: self.data.max_locals,
            returns: self.data.max_returns,
            branches: self.data.max_branches,
            statements: self.data.max_statements,
            boolean: self.data.max_bool_expr,
        }
    }
    pub(super) fn mutable_default(&self, qname: &str) -> Option<&str> {
        self.data
            .mutable_default_symbols
            .get(qname)
            .map(String::as_str)
    }
    pub(super) fn protected_excluded(&self, name: &str) -> bool {
        self.data.exclude_protected.contains(name)
    }
    pub(super) fn protected_special_methods(&self) -> bool {
        self.data.check_protected_access_in_special_methods
    }
    pub(super) fn special_method(&self, name: &str) -> bool {
        self.data.special_method_parameters.contains_key(name)
    }
    pub(super) fn special_parameters(&self, name: &str) -> Option<&SpecialParameters> {
        self.data
            .special_method_parameters
            .get(name)
            .and_then(Option::as_ref)
    }
    pub(super) fn design_limits(&self) -> (usize, usize) {
        (self.data.max_parents, self.data.max_attributes)
    }
    pub(super) fn ignored_parent(&self, name: &str) -> bool {
        self.data.ignored_parents.contains(name)
            || self.data.stdlib_classes_ignore_ancestor.contains(name)
    }
    pub(super) fn defining_method(&self, name: &str) -> bool {
        self.data.defining_attr_methods.contains(name)
    }
    pub(super) fn data_decorator(&self, name: &str) -> bool {
        self.data.dataclasses_decorators.contains(name) || self.data.attrs_decorators.contains(name)
    }
    pub(super) fn ignored_mixin(&self, name: &str, rule: &str) -> bool {
        self.mixin.is_match(name) && self.data.ignored_checks_for_mixins.contains(rule)
    }
    pub(super) fn invalid_base(&self, name: &str) -> bool {
        self.data.invalid_base_classes.contains(name)
    }
    pub(super) fn public_limits(&self) -> (usize, usize) {
        (self.data.min_public_methods, self.data.max_public_methods)
    }
    pub(super) fn class_receiver(&self, name: &str) -> bool {
        self.data
            .valid_classmethod_first_arg
            .iter()
            .any(|value| value == name)
    }
    pub(super) fn ignored(&self, name: &str, argument: bool) -> bool {
        if argument {
            self.arguments.is_match(name)
        } else {
            self.dummy.is_match(name)
        }
    }
    pub(super) fn builtin_allowed(&self, name: &str, module: Option<&str>) -> bool {
        self.data.allowed_redefined_builtins.contains(name)
            || module.is_some_and(|module| self.data.redefining_builtins_modules.contains(module))
    }
    pub(super) fn module_attribute(&self, name: &str) -> bool {
        self.data.module_scope_attrs.contains(name)
    }
    pub(super) fn callback(&self, name: &str) -> bool {
        self.data
            .callbacks
            .iter()
            .any(|prefix| name.starts_with(prefix) || name.ends_with(prefix))
    }
    pub(super) fn good(&self, name: &str) -> bool {
        self.data.good_names.contains(name)
    }
    pub(super) fn valid_name(&self, name: &str, kind: &str) -> bool {
        self.good(name)
            || self
                .naming
                .get(kind)
                .expect("locked name category")
                .is_match(name)
    }
}
