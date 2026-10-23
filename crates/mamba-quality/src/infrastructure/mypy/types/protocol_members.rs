//! Structural requirements omit members inherited from the universal object base.
use super::*;
use std::collections::BTreeSet;

impl Context {
    pub(super) fn variance_members(&self, id: u32) -> BTreeSet<String> {
        let mut members = BTreeSet::new();
        for id in self.class_lineage(id) {
            let class = self.class(id);
            if class.namespace == "<builtins>" && class.offset == 0 {
                continue;
            }
            members.extend(class.fields.keys().cloned());
            members.extend(
                class
                    .methods
                    .keys()
                    .filter(|name| {
                        !matches!(name.as_str(), "__init__" | "__new__" | "__init_subclass__")
                    })
                    .cloned(),
            );
        }
        members
    }

    pub(in crate::infrastructure::mypy) fn protocol_members(&self, id: u32) -> BTreeSet<String> {
        let mut members = BTreeSet::new();
        for id in self.class_lineage(id) {
            let class = self.class(id);
            if !class.protocol {
                continue;
            }
            members.extend(
                class
                    .fields
                    .keys()
                    .chain(class.methods.keys())
                    .filter(|name| {
                        !matches!(
                            name.as_str(),
                            "__abstractmethods__"
                                | "__annotations__"
                                | "__dict__"
                                | "__doc__"
                                | "__init__"
                                | "__module__"
                                | "__new__"
                                | "__slots__"
                                | "__subclasshook__"
                                | "__weakref__"
                                | "__class_getitem__"
                        )
                    })
                    .cloned(),
            );
        }
        members
    }
}
