//! Black-profile output for the pinned isort ordering and comment model.
use super::model::{add_comment, module_key, name_key, natural_cmp, Binding, Import};
use crate::domain::source::PythonVersion;

pub(super) fn binding(binding: &Binding) -> String {
    match &binding.alias {
        Some(alias) => format!("{} as {alias}", binding.name),
        None => binding.name.clone(),
    }
}

fn append_line(output: &mut String, line: &str, comment: &str, newline: &str) {
    output.push_str(line);
    if !comment.is_empty() {
        output.push_str("  ");
        output.push_str(comment.trim());
    }
    output.push_str(newline);
}

fn emit_from(
    output: &mut String,
    import: &Import,
    names: &[Binding],
    indent: &str,
    newline: &str,
    explode: bool,
) {
    if names.is_empty() {
        return;
    }
    let header = format!("{indent}from {} import ", import.module);
    let joined = names.iter().map(binding).collect::<Vec<_>>().join(", ");
    let comment_width = if import.comment.is_empty() {
        0
    } else {
        2 + import.comment.chars().count()
    };
    let width = header.chars().count() + joined.chars().count() + comment_width;
    if (width > 88 || explode) && names[0].name != "*" {
        append_line(output, &format!("{header}("), &import.comment, newline);
        for name in names {
            append_line(
                output,
                &format!("{indent}    {},", binding(name)),
                &name.comment,
                newline,
            );
        }
        append_line(output, &format!("{indent})"), "", newline);
    } else {
        append_line(
            output,
            &format!("{header}{joined}"),
            &import.comment,
            newline,
        );
    }
}

fn emit_single(output: &mut String, import: &Import, name: &Binding, indent: &str, newline: &str) {
    let header = format!("{indent}from {} import ", import.module);
    let attribute = binding(name);
    let mut comment = name.comment.clone();
    if !name.opening_comment.is_empty() {
        if !comment.is_empty() {
            comment.push_str("  ");
        }
        comment.push_str(&name.opening_comment);
    }
    let width = header.chars().count()
        + attribute.chars().count()
        + if name.comment.is_empty() {
            0
        } else {
            name.comment.chars().count() + 2
        };
    if width <= 88 || name.name == "*" {
        append_line(output, &format!("{header}{attribute}"), &comment, newline);
    } else {
        let hoist = name.comment.contains("noqa");
        let mut opening = if hoist {
            name.comment.clone()
        } else {
            String::new()
        };
        if !name.opening_comment.is_empty() {
            if !opening.is_empty() {
                opening.push_str("  ");
            }
            opening.push_str(&name.opening_comment);
        }
        let nested = if hoist { "" } else { &name.comment };
        append_line(output, &format!("{header}("), &opening, newline);
        append_line(
            output,
            &format!("{indent}    {attribute},"),
            nested,
            newline,
        );
        append_line(output, &format!("{indent})"), "", newline);
    }
}

fn merge(imports: Vec<Import>) -> Vec<Import> {
    let mut merged: Vec<Import> = Vec::new();
    for mut import in imports {
        let mut unique: Vec<Binding> = Vec::new();
        for name in import.names.drain(..) {
            if let Some(previous) = unique
                .iter_mut()
                .find(|previous| previous.name == name.name && previous.alias == name.alias)
            {
                if !name.comment.is_empty() {
                    previous.comment = name.comment;
                }
                if !name.opening_comment.is_empty() {
                    add_comment(&mut previous.opening_comment, &name.opening_comment);
                }
            } else {
                unique.push(name);
            }
        }
        import.names = unique;
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.from == import.from && existing.module == import.module)
        {
            let mut leading = import.leading;
            leading.extend(std::mem::take(&mut existing.leading));
            existing.leading = leading;
            if !import.comment.is_empty() {
                add_comment(&mut existing.comment, &import.comment);
            }
            existing.multiline |= import.multiline;
            for name in import.names {
                if let Some(previous) = existing
                    .names
                    .iter_mut()
                    .find(|previous| previous.name == name.name && previous.alias == name.alias)
                {
                    if !name.comment.is_empty() {
                        previous.comment = name.comment;
                    }
                    if !name.opening_comment.is_empty() {
                        add_comment(&mut previous.opening_comment, &name.opening_comment);
                    }
                } else {
                    existing.names.push(name);
                }
            }
        } else {
            merged.push(import);
        }
    }
    merged
}

fn from_import(
    output: &mut String,
    import: &Import,
    indent: &str,
    newline: &str,
    version: PythonVersion,
) {
    let mut names = import.names.clone();
    names.sort_by(|left, right| {
        natural_cmp(
            &name_key(&left.name, version),
            &name_key(&right.name, version),
            version,
        )
    });
    if let Some(index) = names.iter().position(|name| name.name == "*") {
        let star = names.remove(index);
        emit_single(output, import, &star, indent, newline);
    }
    let mut comments = import.comment.clone();
    while !names.is_empty() {
        let mut processed_aliases = false;
        while let Some(first) = names.first() {
            let name = first.name.clone();
            if !names
                .iter()
                .any(|binding| binding.name == name && binding.alias.is_some())
            {
                break;
            }
            processed_aliases = true;
            let count = names
                .iter()
                .take_while(|binding| binding.name == name)
                .count();
            let mut aliases: Vec<_> = names.drain(..count).collect();
            aliases.sort_by(|left, right| {
                left.alias
                    .is_some()
                    .cmp(&right.alias.is_some())
                    .then_with(|| natural_cmp(&binding(left), &binding(right), version))
            });
            let mut opening = String::new();
            for alias in &mut aliases {
                if !alias.opening_comment.is_empty() {
                    add_comment(&mut opening, &alias.opening_comment);
                    alias.opening_comment.clear();
                }
            }
            if let Some(first) = aliases.first_mut() {
                if first.alias.is_none() {
                    if !first.comment.is_empty() {
                        add_comment(&mut opening, &first.comment);
                    }
                    first.comment = opening;
                } else {
                    first.opening_comment = opening;
                }
            }
            for alias in aliases {
                emit_single(output, import, &alias, indent, newline);
            }
        }
        let mut index = 0;
        while index < names.len() {
            let name = &names[index];
            if name.comment.is_empty()
                || names
                    .iter()
                    .any(|other| other.name == name.name && other.alias.is_some())
            {
                index += 1;
                continue;
            }
            let mut name = names.remove(index);
            if names.is_empty() && !comments.is_empty() {
                let mut combined = comments.clone();
                add_comment(&mut combined, &name.comment);
                name.comment = combined;
                comments.clear();
            }
            emit_single(output, import, &name, indent, newline);
        }
        let end = names
            .iter()
            .position(|name| {
                names
                    .iter()
                    .any(|other| other.name == name.name && other.alias.is_some())
            })
            .unwrap_or(names.len());
        let group: Vec<_> = names.drain(..end).collect();
        let mut grouped = import.clone();
        grouped.comment = comments.clone();
        emit_from(
            output,
            &grouped,
            &group,
            indent,
            newline,
            import.multiline && !processed_aliases,
        );
        comments.clear();
    }
}

pub fn block(imports: Vec<Import>, indent: &str, newline: &str, version: PythonVersion) -> String {
    let mut merged = merge(imports);
    merged.sort_by(|left, right| {
        left.section
            .cmp(&right.section)
            .then_with(|| {
                if left.section == 0 {
                    right.from.cmp(&left.from)
                } else {
                    left.from.cmp(&right.from)
                }
            })
            .then_with(|| {
                natural_cmp(
                    &module_key(&left.module, version),
                    &module_key(&right.module, version),
                    version,
                )
            })
    });
    let mut output = String::new();
    let mut section = None;
    for import in merged {
        if section.is_some_and(|previous| previous != import.section) {
            output.push_str(newline);
        }
        section = Some(import.section);
        if !import.leading.is_empty()
            && !output.is_empty()
            && !output.ends_with(&format!("{newline}{newline}"))
        {
            output.push_str(newline);
        }
        for leading in &import.leading {
            append_line(&mut output, leading, "", newline);
        }
        if import.from {
            from_import(&mut output, &import, indent, newline, version);
        } else {
            let mut names: Vec<_> = import.names.iter().collect();
            names.sort_by_key(|name| name.alias.is_some());
            for name in names {
                append_line(
                    &mut output,
                    &format!("{indent}import {}", binding(name)),
                    &name.comment,
                    newline,
                );
            }
        }
    }
    output
}
