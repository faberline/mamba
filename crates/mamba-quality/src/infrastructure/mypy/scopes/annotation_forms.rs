//! Canonical annotation forms follow live values, not cached variable snapshots.
use super::*;
use std::collections::BTreeSet;

pub(super) struct ImportForm {
    pub origin: Option<(String, String)>,
    pub live: bool,
    pub checked: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ModuleProof {
    module: String,
    origin: Option<(String, String)>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Form {
    marker: Type,
    source: Source,
}

#[derive(Clone, PartialEq, Eq)]
enum Source {
    Import {
        module: String,
        member: String,
    },
    Alias {
        root: String,
        owner: usize,
        definition: TextSize,
        path: Option<String>,
    },
}

impl Scopes {
    pub(in crate::infrastructure::mypy) fn clear_annotation_form(&mut self, name: &str) {
        let index = self
            .annotation_binding_identity(name)
            .map(|(owner, _)| owner)
            .unwrap_or_else(|| self.binding_scope(name));
        if let Some(binding) = self.stack[index].bindings.get_mut(name) {
            binding.annotation_form = None;
            binding.annotation_module = None;
        }
    }

    fn annotation_binding_identity(&self, name: &str) -> Option<(usize, &Binding)> {
        let binding = self.resolved_binding(name)?;
        self.stack
            .iter()
            .position(|scope| {
                scope
                    .bindings
                    .get(name)
                    .is_some_and(|value| std::ptr::eq(value, binding))
            })
            .map(|owner| (owner, binding))
    }

    pub(super) fn import_callable_form(
        &self,
        alias: Option<Type>,
        origin: Option<(String, String)>,
    ) -> Option<Form> {
        let marker = alias.filter(|&ty| {
            self.context.is_callable_form(ty)
                || origin
                    .as_ref()
                    .is_some_and(|(module, member)| self.self_import_marker(ty, module, member))
                || origin.as_ref().is_some_and(|(module, member)| {
                    self.literal_import_export(ty, module, member).is_some()
                })
        })?;
        let (module, member) = origin?;
        Some(Form {
            marker,
            source: Source::Import { module, member },
        })
    }

    pub(super) fn annotation_import_refresh_live(
        &self,
        name: &str,
        definition: TextSize,
        export: &super::super::modules::Export,
        origin: Option<&(String, String)>,
        checked: bool,
    ) -> bool {
        let binding = self.stack[self.binding_scope(name)].bindings.get(name);
        let Some(binding) = binding else {
            return !checked;
        };
        if binding.first_definition != definition {
            return false;
        }
        if !checked {
            return true;
        }
        let module = self.live_annotation_module(binding).is_some_and(|module| {
            export.annotation_module && export.module.as_deref() == Some(module.as_str())
        });
        let form = binding.annotation_form.as_ref().is_some_and(|form| {
            matches!(&form.source, Source::Import { module, member }
                if origin.is_some_and(|(actual_module, actual_member)| module == actual_module && member == actual_member))
                && export.alias == Some(form.marker)
                && (self.context.is_callable_form(form.marker)
                    || self.self_import_binding_matches(binding, form, export)
                    || self.literal_import_binding_matches(binding, form, export))
        });
        module || form
    }

    fn self_import_marker(&self, marker: Type, module: &str, member: &str) -> bool {
        matches!(module, "typing" | "typing_extensions")
            && member == "Self"
            && self.context.node(marker).is_some_and(
                |node| matches!(node.as_ref(), types::Node::TypingForm(form) if form == "Self"),
            )
    }

    fn self_import_binding_matches(
        &self,
        binding: &Binding,
        form: &Form,
        export: &super::super::modules::Export,
    ) -> bool {
        let Source::Import { module, member } = &form.source else {
            return false;
        };
        self.self_import_marker(form.marker, module, member)
            && !binding.infer_from_assignment
            && binding.callable.is_none()
            && binding.module.is_none()
            && binding.annotation_module.is_none()
            && export.alias == Some(form.marker)
            && !export.annotation_module
            && binding.ty == export.ty
            && binding.callable == export.callable
            && binding.module == export.module
    }

    // Only the live module import receipt proves this role. A cached Variables
    // marker or a runtime Any value cannot create it. The lookup rechecks the
    // selected export and can use the existing native provider.
    pub(super) fn live_module_self_form(&self, name: &str) -> Option<Type> {
        let scope = self.current();
        let lookup = self.modules.as_ref()?;
        if self.stack.len() != 1
            || scope.kind != Kind::Module
            || lookup.namespace() != self.namespace
            || scope.redirects.contains_key(name)
        {
            return None;
        }
        let binding = scope.bindings.get(name)?;
        let form = binding.annotation_form.as_ref()?;
        let Source::Import { module, member } = &form.source else {
            return None;
        };
        if !matches!(module.as_str(), "typing" | "typing_extensions") || member != "Self" {
            return None;
        }
        let export = lookup.named_import_member(module, member)?;
        self.self_import_binding_matches(binding, form, &export)
            .then_some(form.marker)
    }

    fn is_literal_marker(&self, marker: Type) -> bool {
        matches!(
            self.context.node(marker).as_deref(),
            Some(types::Node::LiteralForm)
        )
    }

    fn literal_import_marker(&self, marker: Type, module: &str, member: &str) -> bool {
        matches!(module, "typing" | "typing_extensions")
            && member == "Literal"
            && self.is_literal_marker(marker)
    }

    fn literal_import_export(
        &self,
        marker: Type,
        module: &str,
        member: &str,
    ) -> Option<super::super::modules::Export> {
        if !self.is_literal_marker(marker) {
            return None;
        }
        let lookup = self.modules.as_ref()?;
        let export = lookup.named_import_member(module, member)?;
        if export.alias != Some(marker)
            || (!self.literal_import_marker(marker, module, member)
                && (export.is_hidden() || lookup.exports(module)?.get(member) != Some(&export)))
        {
            return None;
        }
        Some(export)
    }

    fn literal_import_binding_matches(
        &self,
        binding: &Binding,
        form: &Form,
        export: &super::super::modules::Export,
    ) -> bool {
        let Source::Import { module, member } = &form.source else {
            return false;
        };
        self.literal_import_export(form.marker, module, member)
            .as_ref()
            == Some(export)
            && !binding.infer_from_assignment
            && binding.callable.is_none()
            && binding.module.is_none()
            && binding.annotation_module.is_none()
            && export.alias == Some(form.marker)
            && !export.annotation_module
            && binding.ty == export.ty
            && binding.callable == export.callable
            && binding.module == export.module
    }

    // Both cached maps must agree with the live import and selected export.
    // A consuming reexport cannot publish this role again.
    pub(super) fn canonical_literal_form(
        &self,
        expression: &Expr,
        variables: &types::Variables,
    ) -> Option<Type> {
        let Expr::Name(name) = expression else {
            return None;
        };
        if !name.ctx.is_load() {
            return None;
        }
        self.live_literal_form_name(name.id.as_str(), variables)
    }

    fn live_literal_form_name(&self, name: &str, variables: &types::Variables) -> Option<Type> {
        let scope = self.current();
        let lookup = self.modules.as_ref()?;
        if self.stack.len() != 1
            || scope.kind != Kind::Module
            || lookup.namespace() != self.namespace
            || scope.redirects.contains_key(name)
        {
            return None;
        }
        let marker = *variables.get(name)?;
        let binding = scope.bindings.get(name)?;
        let form = binding.annotation_form.as_ref()?;
        let Source::Import { module, member } = &form.source else {
            return None;
        };
        if form.marker != marker
            || scope.variables.get(name).copied() != Some(marker)
            || !self.is_literal_marker(marker)
        {
            return None;
        }
        let export = lookup.named_import_member(module, member)?;
        self.literal_import_binding_matches(binding, form, &export)
            .then_some(marker)
    }

    pub(super) fn import_annotation_module(
        &self,
        export: &super::super::modules::Export,
        origin: Option<&(String, String)>,
    ) -> Option<ModuleProof> {
        if !export.annotation_module {
            return None;
        }
        Some(ModuleProof {
            module: export.module.clone()?,
            origin: origin.cloned(),
        })
    }

    pub(super) fn live_annotation_module(&self, binding: &Binding) -> Option<String> {
        let proof = binding.annotation_module.as_ref()?;
        if binding.module.as_deref() != Some(proof.module.as_str()) {
            return None;
        }
        let lookup = self.modules.as_ref()?;
        let proved = if let Some((module, member)) = &proof.origin {
            lookup
                .named_import_member(module, member)
                .is_some_and(|export| {
                    export.annotation_module
                        && export.module.as_deref() == Some(proof.module.as_str())
                })
        } else {
            lookup.annotation_module_exists(&proof.module)
        };
        proved.then(|| proof.module.clone())
    }

    pub(super) fn export_annotation_module(&self, name: &str, binding: &Binding) -> bool {
        self.resolved_binding(name)
            .is_some_and(|current| std::ptr::eq(current, binding))
            && self.live_annotation_module(binding).is_some()
    }

    fn live_callable_form_name(
        &self,
        name: &str,
        active: &mut BTreeSet<(usize, String)>,
    ) -> Option<Type> {
        let (owner, binding) = self.annotation_binding_identity(name)?;
        let form = binding
            .annotation_form
            .as_ref()
            .filter(|form| self.context.is_callable_form(form.marker))?;
        let key = (owner, name.to_owned());
        if !active.insert(key.clone()) {
            return None;
        }
        let current = match &form.source {
            Source::Import { module, member } => self
                .modules
                .as_ref()
                .and_then(|lookup| lookup.named_import_member(module, member))
                .and_then(|export| export.alias),
            Source::Alias {
                root,
                owner,
                definition,
                path,
            } => self
                .annotation_binding_identity(root)
                .filter(|(current_owner, value)| {
                    current_owner == owner && value.first_definition == *definition
                })
                .and_then(|(_, value)| match path {
                    Some(path) => self.live_annotation_module(value).and_then(|module| {
                        self.modules
                            .as_ref()?
                            .canonical_callable_annotation(&module, path)
                    }),
                    None => self.live_callable_form_name(root, active),
                }),
        };
        active.remove(&key);
        current.filter(|&ty| ty == form.marker && self.context.is_callable_form(ty))
    }

    pub(super) fn live_callable_form_path(&self, name: &str) -> Option<Type> {
        let root = name.split('.').next()?;
        if self.current().variables.get(root).is_some_and(|&ty| {
            matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Variable(_))
            )
        }) {
            return None;
        }
        if let Some((root, path)) = name.split_once('.') {
            let (_, binding) = self.annotation_binding_identity(root)?;
            let module = self.live_annotation_module(binding)?;
            let ty = self
                .modules
                .as_ref()?
                .canonical_callable_annotation(&module, path)?;
            self.context.is_callable_form(ty).then_some(ty)
        } else {
            // A temporary annotation frame may have a distinct local binder.
            // A cached map may reject proof; it can never create proof.
            let &annotation = self.current().variables.get(name)?;
            if !self.context.is_callable_form(annotation)
                && !self.context.alias_id(annotation).is_some_and(|id| {
                    self.resolved_binding(name)
                        .and_then(|binding| binding.ty)
                        .and_then(|ty| self.context.alias_id(ty))
                        == Some(id)
                })
            {
                return None;
            }
            self.live_callable_form_name(name, &mut BTreeSet::new())
        }
    }

    // A later binder consumer must use this live Scope query, not a Signature
    // shape or an unchecked Variables snapshot. No binder change occurs here.
    pub(in crate::infrastructure::mypy) fn canonical_callable_form(
        &self,
        expression: &Expr,
    ) -> Option<Type> {
        let name = types::Context::annotation_form_name(expression)?;
        if self.current().variables.get(&name).is_some_and(|&ty| {
            matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Variable(_))
            )
        }) {
            return None;
        }
        self.live_callable_form_path(&name)
    }

    pub(super) fn alias_callable_form(&self, expression: &Expr) -> Option<Form> {
        let name = types::Context::annotation_form_name(expression)?;
        let marker = self.canonical_callable_form(expression)?;
        let (root, path) = name
            .split_once('.')
            .map_or((name.as_str(), None), |(root, path)| {
                (root, Some(path.to_owned()))
            });
        let (owner, binding) = self.annotation_binding_identity(root)?;
        Some(Form {
            marker,
            source: Source::Alias {
                root: root.into(),
                owner,
                definition: binding.first_definition,
                path,
            },
        })
    }

    pub(super) fn live_annotation_variables(
        &self,
        namespace: &types::Variables,
    ) -> types::Variables {
        let mut variables = namespace.clone();
        for (name, &ty) in namespace.iter() {
            let root = name.split('.').next().unwrap();
            let shadowed = namespace.get(root).is_some_and(|&ty| {
                matches!(
                    self.context.node(ty).as_deref(),
                    Some(types::Node::Variable(_))
                )
            });
            if self.context.is_callable_form(ty) && shadowed {
                variables.remove(name);
                continue;
            }
            if self.context.is_callable_form(ty) {
                if let Some(form) = self.live_callable_form_path(name) {
                    if form != ty {
                        variables.insert(name.clone(), form);
                    }
                } else {
                    variables.remove(name);
                }
            } else if !shadowed && self.context.alias_id(ty).is_some() {
                if let Some(form) = self.live_callable_form_path(name) {
                    if form != ty {
                        variables.insert(name.clone(), form);
                    }
                }
            }
        }
        variables
    }

    pub(super) fn export_annotation_alias(&self, name: &str, binding: &Binding) -> Option<Type> {
        if self
            .resolved_binding(name)
            .is_some_and(|current| std::ptr::eq(current, binding))
        {
            if self
                .current()
                .variables
                .get(name)
                .is_some_and(|&ty| self.is_literal_marker(ty))
            {
                let form = binding.annotation_form.as_ref()?;
                let Source::Import { module, member } = &form.source else {
                    return None;
                };
                // Only direct canonical imports can export Literal proof.
                if !self.literal_import_marker(form.marker, module, member) {
                    return None;
                }
                return self.live_literal_form_name(name, &self.current().variables);
            }
            if let Some(form) = self.live_callable_form_name(name, &mut BTreeSet::new()) {
                return Some(form);
            }
        }
        self.current()
            .variables
            .get(name)
            .copied()
            .filter(|&ty| !self.context.is_callable_form(ty) && !self.is_literal_marker(ty))
            // The queried name must be this exact exporting binding.
            .filter(|_| {
                self.current()
                    .bindings
                    .get(name)
                    .is_some_and(|current| std::ptr::eq(current, binding))
            })
    }
}
