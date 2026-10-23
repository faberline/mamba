//! Iteration contexts keep typed observations until all real visits are known.
use super::*;
use ruff_text_size::{TextRange, TextSize};
use types::Type;

type Span = (u32, u32);

#[derive(Clone, PartialEq, Eq)]
enum TypedEvent {
    Cast(Span, String),
    Overlap(Span, &'static str),
    Reveal(Span),
}

fn span(range: TextRange) -> Span {
    (range.start().to_u32(), range.end().to_u32())
}

fn range(span: Span) -> TextRange {
    TextRange::new(TextSize::new(span.0), TextSize::new(span.1))
}

#[derive(Default, Clone)]
struct Pass {
    checked: BTreeSet<Span>,
    overlaps: BTreeMap<(Span, &'static str), (Type, Type)>,
    reveals: BTreeMap<Span, Vec<(Type, Option<String>)>>,
    casts: BTreeMap<(Span, String), Cast>,
}

#[derive(Clone)]
struct Cast {
    expected: Type,
    message: String,
    location: TextRange,
    anchor: Span,
}

#[derive(Clone)]
pub(in crate::infrastructure::mypy::checker) struct IterationDiagnostics {
    label: &'static str,
    current_start: Option<usize>,
    current: Pass,
    passes: Vec<Pass>,
    order: Vec<TypedEvent>,
    pub(in crate::infrastructure::mypy::checker) ordinary: Vec<std::ops::Range<usize>>,
}

impl IterationDiagnostics {
    pub(in crate::infrastructure::mypy::checker) fn new(label: &'static str) -> Self {
        Self {
            label,
            current_start: None,
            current: Pass::default(),
            passes: Vec::new(),
            order: Vec::new(),
            ordinary: Vec::new(),
        }
    }

    fn observe(&mut self, event: TypedEvent) {
        if !self.order.contains(&event) {
            self.order.push(event);
        }
    }

    fn begin_context(&mut self, first: usize) {
        assert!(
            self.current_start.is_none(),
            "iteration diagnostic context already active"
        );
        self.current_start = Some(first);
    }

    fn finish_context(&mut self, end: usize) {
        let first = self
            .current_start
            .take()
            .expect("active iteration diagnostic context");
        assert!(first <= end, "iteration diagnostic range is reversed");
        self.passes.push(std::mem::take(&mut self.current));
        self.ordinary.push(first..end);
    }

    pub(in crate::infrastructure::mypy::checker) fn remap_diagnostic_ranges(
        &mut self,
        removed: &BTreeSet<usize>,
    ) {
        if let Some(first) = &mut self.current_start {
            *first -= removed.range(..*first).count();
        }
        for range in &mut self.ordinary {
            let start = range.start;
            let end = range.end;
            range.start -= removed.range(..start).count();
            range.end -= removed.range(..end).count();
        }
    }
}

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn begin_iteration_diagnostics(
        &mut self,
        label: &'static str,
    ) -> usize {
        self.iteration_diagnostics
            .push(IterationDiagnostics::new(label));
        self.iteration_diagnostics.len()
    }

    pub(in crate::infrastructure::mypy::checker) fn begin_iteration_context(
        &mut self,
        depth: usize,
    ) {
        assert_eq!(
            self.iteration_diagnostics.len(),
            depth,
            "iteration diagnostic collectors are not balanced"
        );
        let first = self.diagnostics.len();
        self.iteration_diagnostics
            .last_mut()
            .expect("active iteration diagnostic collector")
            .begin_context(first);
    }

    pub(in crate::infrastructure::mypy::checker) fn finish_iteration_context(
        &mut self,
        depth: usize,
    ) {
        assert_eq!(
            self.iteration_diagnostics.len(),
            depth,
            "iteration diagnostic collectors are not balanced"
        );
        let end = self.diagnostics.len();
        self.iteration_diagnostics
            .last_mut()
            .expect("active iteration diagnostic collector")
            .finish_context(end);
    }

    pub(in crate::infrastructure::mypy::checker) fn observe_loop_expression(
        &mut self,
        value: &Expr,
    ) {
        let position = span(value.range());
        // An inner collector owns its typed errors. Its visited sites also belong
        // to the enclosing pass, so a later forwarded result has a checked site.
        for frame in &mut self.iteration_diagnostics {
            assert!(
                frame.current_start.is_some(),
                "typed observation needs an active iteration diagnostic context"
            );
            frame.current.checked.insert(position);
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn loop_overlap(
        &mut self,
        kind: &'static str,
        left: Type,
        right: Type,
        location: TextRange,
        original: bool,
    ) -> bool {
        if self.typed_dict_trial_failed.is_some() {
            return false;
        }
        if !self.iteration_diagnostics.is_empty() && !original {
            let label = self.iteration_diagnostics.last().unwrap().label;
            self.loop_gaps.insert(format!(
                "{label} comparison has no original source event at byte {}",
                location.start().to_u32()
            ));
            return false;
        }
        let Some(frame) = self.iteration_diagnostics.last_mut() else {
            return false;
        };
        assert!(
            frame.current_start.is_some(),
            "typed event needs an active iteration diagnostic context"
        );
        frame.observe(TypedEvent::Overlap(span(location), kind));
        frame
            .current
            .overlaps
            .insert((span(location), kind), (left, right));
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn loop_reveal(
        &mut self,
        ty: Type,
        override_name: Option<String>,
        location: TextRange,
        original: bool,
    ) -> bool {
        if !self.iteration_diagnostics.is_empty() && !original {
            let label = self.iteration_diagnostics.last().unwrap().label;
            self.loop_gaps.insert(format!(
                "{label} reveal has no original source event at byte {}",
                location.start().to_u32()
            ));
            return false;
        }
        let Some(frame) = self.iteration_diagnostics.last_mut() else {
            return false;
        };
        assert!(
            frame.current_start.is_some(),
            "typed event needs an active iteration diagnostic context"
        );
        frame.observe(TypedEvent::Reveal(span(location)));
        frame
            .current
            .reveals
            .entry(span(location))
            .or_default()
            .push((ty, override_name));
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn loop_redundant_cast(
        &mut self,
        expected: Type,
        message: String,
        location: TextRange,
        checked_call: TextRange,
        original: bool,
    ) -> bool {
        if self.typed_dict_trial_failed.is_some() {
            return false;
        }
        if !self.iteration_diagnostics.is_empty() && !original {
            let label = self.iteration_diagnostics.last().unwrap().label;
            self.loop_gaps.insert(format!(
                "{label} cast has no original source event at byte {}",
                checked_call.start().to_u32()
            ));
            return false;
        }
        let Some(frame) = self.iteration_diagnostics.last_mut() else {
            return false;
        };
        assert!(
            frame.current_start.is_some(),
            "typed event needs an active iteration diagnostic context"
        );
        frame.observe(TypedEvent::Cast(span(location), message.clone()));
        frame.current.casts.insert(
            (span(location), message.clone()),
            Cast {
                expected,
                message,
                location,
                anchor: span(checked_call),
            },
        );
        true
    }

    pub(in crate::infrastructure::mypy::checker) fn settle_iteration_diagnostics(
        &mut self,
        depth: usize,
    ) {
        assert_eq!(
            self.iteration_diagnostics.len(),
            depth,
            "iteration diagnostic collectors are not balanced"
        );
        let frame = self
            .iteration_diagnostics
            .last()
            .expect("active iteration diagnostic collector");
        assert!(
            frame.current_start.is_none(),
            "iteration diagnostic context is still active"
        );
        assert!(
            !frame.passes.is_empty(),
            "iteration diagnostic collector has no completed context"
        );
        let frame = self
            .iteration_diagnostics
            .pop()
            .expect("active iteration diagnostic collector");
        self.compact_loop_pass_diagnostics(&frame.ordinary);
        // The original releases uselessness, overlap and reveal groups in this
        // order. Within a group retain first observation order, never message
        // order. Notes stay complete records with their original source bounds.
        let casts: Vec<_> = frame
            .order
            .iter()
            .filter_map(|event| match event {
                TypedEvent::Cast(position, message) => Some((*position, message.clone())),
                _ => None,
            })
            .collect();
        let mut persistent_casts = BTreeSet::new();
        for key in casts {
            let first = frame
                .passes
                .iter()
                .find_map(|pass| pass.casts.get(&key))
                .expect("cast came from a recorded pass");
            if frame
                .passes
                .iter()
                .any(|pass| !pass.casts.contains_key(&key))
            {
                continue;
            }
            if !persistent_casts.insert(key.0) {
                self.loop_gaps.insert(format!(
                    "{} cast order has multiple persistent messages at byte {}",
                    frame.label, key.0 .0
                ));
            }
            if !self.loop_redundant_cast(
                first.expected,
                first.message.clone(),
                first.location,
                range(first.anchor),
                true,
            ) {
                self.emit("redundant-cast", first.message.clone(), &first.location);
            }
        }
        let positions: Vec<_> = frame
            .order
            .iter()
            .filter_map(|event| match event {
                TypedEvent::Overlap(position, kind) => Some((*position, *kind)),
                _ => None,
            })
            .collect();
        for (position, kind) in positions {
            // The fixed strict policy does not enable warn_unreachable. The
            // original watcher therefore has no unreachable-line exception.
            // A skipped expression must not invent such an exception here.
            if frame
                .passes
                .iter()
                .any(|pass| !pass.overlaps.contains_key(&(position, kind)))
            {
                continue;
            }
            let inputs: Vec<_> = frame
                .passes
                .iter()
                .filter_map(|pass| pass.overlaps.get(&(position, kind)))
                .copied()
                .collect();
            let left = self
                .context
                .union(inputs.iter().map(|input| input.0).collect());
            let right = self
                .context
                .union(inputs.iter().map(|input| input.1).collect());
            let location = range(position);
            if !self.loop_overlap(kind, left, right, location, true) {
                let (left_label, right_label) = if kind == "container" {
                    ("element", "container item")
                } else {
                    ("left operand", "right operand")
                };
                self.emit("comparison-overlap", format!(
                    "Non-overlapping {kind} check ({left_label} type: \"{}\", {right_label} type: \"{}\")",
                    self.context.name(left), self.context.name(right)
                ), &location);
            }
        }
        let mut reveals: BTreeMap<Span, Vec<(Type, Option<String>)>> = BTreeMap::new();
        for pass in frame.passes {
            for (position, values) in pass.reveals {
                reveals.entry(position).or_default().extend(values);
            }
        }
        for position in frame.order.into_iter().filter_map(|event| match event {
            TypedEvent::Reveal(position) => Some(position),
            _ => None,
        }) {
            let values = reveals.remove(&position).expect("recorded reveal event");
            let ty = self
                .context
                .union(values.iter().map(|value| value.0).collect());
            let override_name = values
                .first()
                .and_then(|value| value.1.clone())
                .filter(|name| values.iter().all(|value| value.1.as_ref() == Some(name)));
            let location = range(position);
            if !self.loop_reveal(ty, override_name.clone(), location, true) {
                self.emit_alias_reveal(ty, override_name, location);
            }
        }
    }
}
