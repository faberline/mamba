//! Full lexical maps share immutable snapshots until a write needs a private map.
use super::Type;
use std::{
    collections::{btree_map, BTreeMap},
    fmt,
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub(crate) type VariableMap = BTreeMap<String, Type>;

#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct Variables(Arc<VariableMap>);

impl Variables {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn as_map(&self) -> &VariableMap {
        &self.0
    }
}

impl fmt::Debug for Variables {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_map(), formatter)
    }
}

impl Deref for Variables {
    type Target = VariableMap;

    fn deref(&self) -> &Self::Target {
        self.as_map()
    }
}

impl DerefMut for Variables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl From<VariableMap> for Variables {
    fn from(variables: VariableMap) -> Self {
        Self(Arc::new(variables))
    }
}

impl<const N: usize> From<[(String, Type); N]> for Variables {
    fn from(variables: [(String, Type); N]) -> Self {
        VariableMap::from(variables).into()
    }
}

impl FromIterator<(String, Type)> for Variables {
    fn from_iter<T: IntoIterator<Item = (String, Type)>>(variables: T) -> Self {
        variables.into_iter().collect::<VariableMap>().into()
    }
}

impl<'a> IntoIterator for &'a Variables {
    type Item = (&'a String, &'a Type);
    type IntoIter = btree_map::Iter<'a, String, Type>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_map().iter()
    }
}

#[cfg(test)]
mod tests;
