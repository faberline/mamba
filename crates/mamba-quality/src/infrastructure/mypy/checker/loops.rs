//! Loop exits merge break paths separately from condition and else paths.
use super::*;
use flow::Flow;
use quality_syntax::python_ast::{StmtFor, StmtWhile};
pub(super) mod iteration;

#[derive(Default, Clone)]
pub(super) struct LoopFrame {
    pub(super) breaks: Vec<Flow>,
    pub(super) continues: Vec<Flow>,
}

impl Checker<'_> {
    pub(super) fn check_for<'a>(&mut self, looped: &'a StmtFor)
    where
        Self: Visitor<'a>,
    {
        let baseline = self.environment();
        self.visit_expr(&looped.iter);
        let iterable = self.infer(&looped.iter);
        let dictionary_tuple = match (looped.target.as_ref(), looped.iter.as_ref(), iterable) {
            (Expr::Name(name), Expr::Tuple(tuple), Some(observed))
                if !looped.is_async
                    && !self.source.is_stub
                    && !self.source.text.contains("type:")
                    && !self.source.text.contains("mypy:") =>
            {
                let owner = self.scopes.try_input_scope();
                let definition = self.scopes.direct_inferred_local_binding(name.id.as_str());
                // Only the original inferred definition can use Mypy's
                // incomplete-variable fallback. Replays retain this identity.
                (name.id.as_str() != "_"
                    && owner.1 == scopes::Kind::Function
                    && owner.2.is_none()
                    && self.scopes.loop_binding_owner(name.id.as_str()) == owner.0
                    && (definition == Some((owner.0, name.start()))
                        || !self.scopes.has_binding(name.id.as_str())))
                .then(|| {
                    self.context
                        .literal_dictionary_tuple_item(tuple, observed)
                        .or_else(|| {
                            self.scopes
                                .loop_target_annotation_role_absent(name.id.as_str())
                                .then(|| {
                                    self.context
                                        .literal_empty_list_dictionary_tuple_item(
                                            &looped.iter,
                                            observed,
                                        )
                                        .or_else(|| {
                                            self.context.literal_object_dictionary_tuple_item(
                                                &looped.iter,
                                                observed,
                                                &baseline,
                                            )
                                        })
                                })
                                .flatten()
                        })
                })
                .flatten()
            }
            _ => None,
        };
        let mut invariant_columns = Vec::new();
        let mut varying_columns = false;
        let item = if looped.is_async {
            iterable.map(|ty| match self.context.async_iterable_item(ty) {
                Ok(item) => item,
                Err(types::AsyncIterationError::MissingIterator) => {
                    self.emit_with_hint(
                        "attr-defined",
                        format!(
                            "\"{}\" has no attribute \"__aiter__\" (not async iterable)",
                            self.context.name(ty)
                        ),
                        looped.iter.as_ref(),
                        self.context.await_result(ty).map(|_| "Maybe you forgot to use \"await\"?".into()),
                    );
                    types::Type::Any
                }
                Err(types::AsyncIterationError::MissingNext(iterator)) => {
                    let message = self.context.missing_builtin_attribute(
                        iterator,
                        "__anext__",
                        self.python_target,
                    ).unwrap_or_else(|| format!(
                        "\"{}\" has no attribute \"__anext__\"",
                        self.context.name(iterator)
                    ));
                    self.emit_with_hint(
                        "attr-defined",
                        message,
                        looped.iter.as_ref(),
                        self.context.await_result(iterator).map(|_| "Maybe you forgot to use \"await\"?".into()),
                    );
                    types::Type::Any
                }
                Err(types::AsyncIterationError::NonAwaitableNext(actual)) => {
                    self.emit(
                        "misc",
                        format!(
                            "Incompatible types in \"async for\" (actual type \"{}\", expected type \"Awaitable[Any]\")",
                            self.context.name(actual)
                        ),
                        looped.iter.as_ref(),
                    );
                    types::Type::Any
                }
            })
        } else {
            iterable.and_then(|ty| {
                let fixed = self.context.node(ty).and_then(|node| {
                    let types::Node::Tuple(items) = node.as_ref() else {
                        return None;
                    };
                    self.fixed_tuple_loop_plan(&looped.target, items)
                        .map(|(item, _)| item)
                });
                if let Some(item) = fixed {
                    return Some(item);
                }
                let item = self.context.iterable_item(ty);
                if let Some(node) = self.context.node(ty) {
                    if let types::Node::Tuple(rows) = node.as_ref() {
                        if let Some((columns, varying)) = self
                            .fixed_tuple_loop_invariant_columns(&looped.target, rows)
                        {
                            if !columns.is_empty() {
                                invariant_columns = columns;
                                varying_columns = varying;
                            }
                        }
                    }
                }
                if item.is_none() && self.context.async_iterable_item(ty).is_ok() {
                    self.emit(
                        "attr-defined",
                        format!(
                            "\"{}\" has no attribute \"__iter__\"; maybe \"__aiter__\"? (not iterable)",
                            self.context.name(ty)
                        ),
                        looped.iter.as_ref(),
                    );
                    Some(types::Type::Any)
                } else {
                    item
                }
            })
        };
        let item = dictionary_tuple.map_or(item, |(item, _)| Some(item));
        if let (Some((_, true)), Expr::Name(name)) = (dictionary_tuple, looped.target.as_ref()) {
            self.emit(
                "var-annotated",
                format!("Need type annotation for \"{}\"", name.id),
                &ruff_text_size::TextRange::at(looped.start(), ruff_text_size::TextSize::new(1)),
            );
        }
        if !invariant_columns.is_empty() && varying_columns {
            self.loop_gaps.insert(format!(
                "Fixed tuple loop keeps varying columns on the legacy fallback in {} at byte {}",
                self.scopes.loop_namespace(),
                looped.start().to_u32()
            ));
        }
        self.check_fixed_tuple_unpack_count(
            &looped.target,
            item,
            &ruff_text_size::TextRange::at(looped.start(), ruff_text_size::TextSize::new(1)),
        );
        let (frame, exhausted) = self.loop_backedges(
            &looped.body,
            iteration::Entry::For {
                target: &looped.target,
                item,
                invariant_columns,
                start: looped.start(),
            },
            looped.range(),
            &baseline,
        );
        self.flow = exhausted;
        self.visit_block(&looped.orelse);
        let mut outcomes = frame.breaks;
        outcomes.push(self.flow.clone());
        self.merge_flow(&outcomes, &baseline);
    }

    // Prove every fixed tuple alternative before any loop binding is written.
    fn fixed_tuple_loop_plan<'a>(
        &self,
        target: &'a Expr,
        members: &[types::Type],
    ) -> Option<(types::Type, Vec<(&'a Expr, types::Type)>)> {
        if members.is_empty()
            || !matches!(target, Expr::Tuple(_) | Expr::List(_))
            || !members
                .iter()
                .all(|&item| self.fixed_tuple_loop_shape(target, item))
        {
            return None;
        }
        // Raw unions do not remove proper subtypes. Admit only identical leaf
        // IDs or direct Int/Float alternatives, whose promotions must be ignored.
        let mut baseline: Option<Vec<(&Expr, types::Type)>> = None;
        for &member in members {
            let bindings = self.scopes.assignment_bindings(target, member)?;
            if let Some(baseline) = &baseline {
                if baseline.len() != bindings.len()
                    || !baseline.iter().zip(&bindings).all(
                        |(&(first_leaf, first), &(leaf, item))| {
                            std::ptr::eq(first_leaf, leaf)
                                && (first == item
                                    || matches!(
                                        (first, item),
                                        (
                                            types::Type::Int | types::Type::Float,
                                            types::Type::Int | types::Type::Float
                                        )
                                    ))
                        },
                    )
                {
                    return self.fixed_tuple_loop_scalar_columns(target, members);
                }
            } else {
                baseline = Some(bindings);
            }
        }
        let item = self.context.union(members.to_vec());
        let bindings = self.scopes.assignment_bindings(target, item)?;
        Some((item, bindings))
    }

    // Only the original full shape proof calls this after raw column IDs differ.
    fn fixed_tuple_loop_scalar_columns<'a>(
        &self,
        target: &'a Expr,
        members: &[types::Type],
    ) -> Option<(types::Type, Vec<(&'a Expr, types::Type)>)> {
        let rows = members
            .iter()
            .map(|&member| self.scopes.assignment_bindings(target, member))
            .collect::<Option<Vec<_>>>()?;
        let first = rows.first()?;
        if rows.iter().any(|row| {
            row.len() != first.len()
                || row
                    .iter()
                    .zip(first)
                    .any(|((leaf, _), (other, _))| !std::ptr::eq(*leaf, *other))
        }) {
            return None;
        }
        let mut bindings = Vec::with_capacity(first.len());
        for (index, &(leaf, ty)) in first.iter().enumerate() {
            let values: Vec<_> = rows.iter().map(|row| row[index].1).collect();
            let column = if values.iter().all(|&item| item == ty) {
                ty
            } else if values.iter().all(|&item| {
                matches!(
                    (ty, item),
                    (
                        types::Type::Int | types::Type::Float,
                        types::Type::Int | types::Type::Float
                    )
                )
            }) {
                self.context.union(values)
            } else {
                self.fixed_tuple_loop_scalar_column(values)?
            };
            bindings.push((leaf, column));
        }
        Some((self.context.union(members.to_vec()), bindings))
    }

    fn fixed_tuple_loop_scalar_column(&self, values: Vec<types::Type>) -> Option<types::Type> {
        // Direct Int/Str columns need no literal or subtype reduction.
        if values
            .iter()
            .all(|&ty| matches!(ty, types::Type::Int | types::Type::Str))
            && values.contains(&types::Type::Int)
            && values.contains(&types::Type::Str)
        {
            return Some(self.context.union(values));
        }
        let plain = |ty| {
            matches!(
                ty,
                types::Type::Bool
                    | types::Type::Int
                    | types::Type::Float
                    | types::Type::Complex
                    | types::Type::Str
                    | types::Type::Bytes
            )
        };
        let scalar = |ty| {
            plain(ty)
                || self.context.node(ty).is_some_and(|node| {
                    matches!(
                        node.as_ref(),
                        types::Node::Literal(_) | types::Node::LiteralValue(_)
                    )
                })
        };
        if !values.iter().all(|&ty| {
            scalar(ty)
                || self.context.node(ty).is_some_and(|node| {
                    matches!(node.as_ref(), types::Node::Union(items)
                    if !items.is_empty() && items.iter().all(|&item| scalar(item)))
                })
        }) {
            return None;
        }
        let joined = self.context.truth_union(values);
        // Require a proved plain result, not an unmodelled literal-union inference.
        plain(joined).then_some(joined)
    }

    fn fixed_tuple_loop_invariant_columns<'a>(
        &self,
        target: &'a Expr,
        rows: &[types::Type],
    ) -> Option<(Vec<(&'a Expr, types::Type)>, bool)> {
        if rows.is_empty()
            || !matches!(target, Expr::Tuple(_) | Expr::List(_))
            || !rows
                .iter()
                .all(|&row| self.fixed_tuple_loop_shape(target, row))
        {
            return None;
        }
        let mut bindings_by_row: Vec<Vec<(&'a Expr, types::Type)>> = Vec::with_capacity(rows.len());
        for &row in rows {
            let bindings = self.scopes.assignment_bindings(target, row)?;
            if let Some(first) = bindings_by_row.first() {
                if first.len() != bindings.len()
                    || !first
                        .iter()
                        .zip(&bindings)
                        .all(|(&(first_leaf, _), &(leaf, _))| std::ptr::eq(first_leaf, leaf))
                {
                    return None;
                }
            }
            bindings_by_row.push(bindings);
        }
        let first = bindings_by_row.first()?;
        let mut invariant = Vec::new();
        let mut varying = false;
        for (index, &(leaf, ty)) in first.iter().enumerate() {
            if bindings_by_row.iter().skip(1).all(|row| row[index].1 == ty) {
                invariant.push((leaf, ty));
            } else {
                varying = true;
            }
        }
        Some((invariant, varying))
    }

    fn fixed_tuple_loop_shape(&self, target: &Expr, item: types::Type) -> bool {
        let targets: &[Expr] = match target {
            Expr::Name(_) => return true,
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            _ => return false,
        };
        self.context.node(item).is_some_and(|node| {
            let types::Node::Tuple(items) = node.as_ref() else {
                return false;
            };
            self.context.tuple_pack_position(items).is_none()
                && items.len() == targets.len()
                && targets
                    .iter()
                    .zip(items)
                    .all(|(target, &item)| self.fixed_tuple_loop_shape(target, item))
        })
    }

    fn bind_loop_target(
        &mut self,
        target: &Expr,
        item: Option<types::Type>,
        start: ruff_text_size::TextSize,
        invariant_columns: &[(&Expr, types::Type)],
    ) {
        let pass = self.begin_try_for_target_pass(target, item.is_some());
        self.bind_loop_target_inner(target, item, start, invariant_columns);
        self.finish_try_for_target_pass(target, pass);
    }

    fn bind_loop_target_inner(
        &mut self,
        target: &Expr,
        item: Option<types::Type>,
        start: ruff_text_size::TextSize,
        invariant_columns: &[(&Expr, types::Type)],
    ) {
        let fixed = item.and_then(|ty| self.context.node(ty)).and_then(|node| {
            let types::Node::Union(members) = node.as_ref() else {
                return None;
            };
            self.fixed_tuple_loop_plan(target, members)
                .map(|(_, bindings)| bindings)
        });
        if let Some(bindings) = fixed {
            for (leaf, item) in bindings {
                self.bind_loop_target_inner(leaf, Some(item), start, invariant_columns);
            }
            return;
        }
        let item = invariant_columns
            .iter()
            .find(|(leaf, _)| std::ptr::eq(*leaf, target))
            .map_or(item, |(_, item)| Some(*item));
        let targets: Option<&[Expr]> = match target {
            Expr::Tuple(tuple) => Some(tuple.elts.as_ref()),
            Expr::List(list) => Some(list.elts.as_ref()),
            _ => None,
        };
        if let Some(targets) = targets {
            let items = item.and_then(|ty| self.context.node(ty));
            for (index, target) in targets.iter().enumerate() {
                let ty = if item.is_some_and(|ty| ty.is_any()) {
                    item.map(types::Type::derived_any)
                } else {
                    items.as_deref().and_then(|node| match node {
                        types::Node::Tuple(items) => items.get(index).copied(),
                        _ => item.and_then(|ty| self.context.iterable_item(ty)),
                    })
                };
                self.bind_loop_target_inner(target, ty, start, invariant_columns);
            }
            return;
        }
        let mut accepted = false;
        let mut no_event = None;
        if let Expr::Name(name) = target {
            let assigned = self.scopes.assigned_type(target);
            let owner = self.scopes.try_input_scope();
            let definition = self.scopes.direct_inferred_local_binding(name.id.as_str());
            // This is declaration inference, not a binder write. Keep opaque
            // and potentially partial items outside this material scalar subset.
            if owner.1 == scopes::Kind::Function
                && owner.2.is_none()
                && name.id.as_str() != "_"
                && self.scopes.loop_binding_owner(name.id.as_str()) == owner.0
                && self.flow.reachable
                && !self.flow.narrowed.unproved()
                && matches!(
                    item,
                    Some(
                        types::Type::Bool
                            | types::Type::Int
                            | types::Type::Float
                            | types::Type::Complex
                            | types::Type::Str
                            | types::Type::Bytes
                    )
                )
                && self
                    .scopes
                    .loop_target_annotation_role_absent(name.id.as_str())
                && (definition == Some((owner.0, name.start()))
                    && self
                        .scopes
                        .inferred_declaration(name.id.as_str(), owner.0, name.start())
                        == item
                    || !self.scopes.has_binding(name.id.as_str()))
                && !self.partial_sets.contains_key(&name.start())
                && !self.partial_defaultdicts.contains_key(&name.start())
                && !self.partial_list_unknown.contains(&name.start())
            {
                no_event = Some(owner);
            }
            if let (Some(expected), Some(actual)) = (assigned, item) {
                let definition = self
                    .scopes
                    .inferred_binding_identity(name.id.as_str())
                    .is_some_and(|(_, origin)| origin == name.start());
                let compatible = self.check_assignment_type(
                    expected,
                    actual,
                    &ruff_text_size::TextRange::at(start, ruff_text_size::TextSize::new(1)),
                );
                // A repeated check of an inferred definition still establishes
                // its declaration. It is not binder.assign_type in the fixed mode.
                accepted = compatible && !definition;
                if !compatible {
                    no_event = None;
                }
            }
            if item == Some(types::Type::Never) && assigned.is_none() {
                self.emit(
                    "var-annotated",
                    format!("Need type annotation for \"{}\"", name.id),
                    &ruff_text_size::TextRange::at(start, ruff_text_size::TextSize::new(1)),
                );
            }
        }
        let raw_item = item;
        let item = item.map(|ty| {
            if ty == types::Type::Never {
                types::Type::Any
            } else {
                ty
            }
        });
        self.scopes.bind(target, item);
        self.flow_assignment(target, item);
        let no_event = no_event.is_some_and(|owner| {
            owner == self.scopes.try_input_scope()
                && self.flow.reachable
                && !self.flow.narrowed.unproved()
                && !self
                    .flow
                    .narrowed
                    .keys()
                    .any(|name| self.scopes.unproved_inferred_none_declaration(name))
                && matches!(target, Expr::Name(name)
                    if self.scopes.direct_inferred_local_binding(name.id.as_str())
                        == Some((owner.0, name.start()))
                    && self.scopes.inferred_declaration(name.id.as_str(), owner.0, name.start())
                        == raw_item)
        });
        if self.complete_try_for_target(target, no_event) {
            return;
        }
        // Without a positive assignment receipt, refuse the active input
        // frame. Binding a fresh inferred declaration alone is not a try event.
        self.record_try_input_commit(target, raw_item, accepted);
    }

    pub(super) fn check_while<'a>(&mut self, looped: &'a StmtWhile)
    where
        Self: Visitor<'a>,
    {
        let baseline = self.environment();
        let (frame, exhausted) = self.loop_backedges(
            &looped.body,
            iteration::Entry::While(&looped.test),
            looped.range(),
            &baseline,
        );
        self.flow = exhausted;
        self.visit_block(&looped.orelse);
        let mut outcomes = frame.breaks;
        outcomes.push(self.flow.clone());
        self.merge_flow(&outcomes, &baseline);
    }

    pub(super) fn check_break(&mut self) {
        if let Some(frame) = self.loops.last_mut() {
            frame.breaks.push(self.flow.clone());
        }
        self.flow.reachable = false;
    }

    pub(super) fn check_continue(&mut self) {
        if let Some(frame) = self.loops.last_mut() {
            frame.continues.push(self.flow.clone());
        }
        self.flow.reachable = false;
    }
}
