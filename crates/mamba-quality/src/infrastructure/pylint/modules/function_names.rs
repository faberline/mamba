use super::Modules;
use mamba_language::application::quality_syntax as syntax;

pub(super) fn is_function(modules: &Modules, project: usize, module: &str, name: &str) -> bool {
    if let Some(targets) = modules.callables.get(&(project, module.into())) {
        if let Some(target) = targets.get(name) {
            return matches!(target, super::super::callables::Target::Function(_));
        }
    }
    if modules.native(module) || modules.project_module(project, module) {
        return false;
    }
    if let Some(target) = modules.call_target(project, module, name) {
        return matches!(target, super::super::callables::Target::Function(_));
    }
    let Some(source) = super::super::super::embedded::module(module) else {
        return false;
    };
    let Some(document) = syntax::parse(source, modules.target, true).ok() else {
        return false;
    };
    let targets = super::super::callables::collect(&document.parsed.syntax().body, module, false);
    matches!(
        targets.get(name),
        Some(super::super::callables::Target::Function(_))
    )
}
