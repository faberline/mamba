use crate::printer::call_stack::PrintElementArgs;
use crate::{FormatElement, TextSize};

/// Stores the queued line suffixes.
#[derive(Debug, Default)]
pub(super) struct LineSuffixes<'a> {
    suffixes: Vec<LineSuffixEntry<'a>>,
    groups: Vec<(std::ops::Range<usize>, Option<TextSize>)>,
}

impl<'a> LineSuffixes<'a> {
    /// Extends the line suffixes with `elements`, storing their call stack arguments with them.
    pub(super) fn extend<I>(&mut self, args: PrintElementArgs, elements: I, order: Option<TextSize>)
    where
        I: IntoIterator<Item = &'a FormatElement>,
    {
        let start = self.suffixes.len();
        self.suffixes
            .extend(elements.into_iter().map(LineSuffixEntry::Suffix));
        self.suffixes.push(LineSuffixEntry::Args(args));
        self.groups.push((start..self.suffixes.len(), order));
    }

    /// Takes all the pending line suffixes.
    pub(super) fn take_pending<'l>(
        &'l mut self,
    ) -> impl DoubleEndedIterator<Item = LineSuffixEntry<'a>> + 'l + ExactSizeIterator {
        if self.groups.iter().all(|(_, order)| order.is_some())
            && self.groups.windows(2).any(|pair| pair[0].1 > pair[1].1)
        {
            self.groups.sort_by_key(|(_, order)| *order);
            let original = std::mem::take(&mut self.suffixes);
            for (range, _) in &self.groups {
                self.suffixes.extend_from_slice(&original[range.clone()]);
            }
        }
        self.groups.clear();
        self.suffixes.drain(..)
    }

    /// Returns `true` if there are any line suffixes and `false` otherwise.
    pub(super) fn has_pending(&self) -> bool {
        !self.suffixes.is_empty()
    }
}

#[derive(Debug, Copy, Clone)]
pub(super) enum LineSuffixEntry<'a> {
    /// A line suffix to print
    Suffix(&'a FormatElement),

    /// Potentially changed call arguments that should be used to format any following items.
    Args(PrintElementArgs),
}
