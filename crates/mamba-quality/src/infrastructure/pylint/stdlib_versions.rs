//! Embedded typeshed lifetimes control standard-library import availability.
use mamba_language::application::quality_syntax::PythonTarget;
use std::{collections::BTreeMap, sync::OnceLock};
fn version(value: &str) -> Option<(u8, u8)> {
    let (major, minor) = value.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}
pub(super) fn available(name: &str, target: PythonTarget) -> bool {
    type Versions = BTreeMap<String, ((u8, u8), Option<(u8, u8)>)>;
    static DATA: OnceLock<Versions> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        include_str!("../../../assets/typeshed/stdlib/VERSIONS")
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next()?.trim();
                let (name, range) = line.split_once(':')?;
                let (minimum, maximum) = range.trim().split_once('-')?;
                Some((
                    name.into(),
                    (
                        version(minimum)?,
                        if maximum.is_empty() {
                            None
                        } else {
                            Some(version(maximum)?)
                        },
                    ),
                ))
            })
            .collect()
    });
    let target = (
        3,
        match target {
            PythonTarget::Py312 => 12,
            PythonTarget::Py313 => 13,
            PythonTarget::Py314 => 14,
        },
    );
    let mut parent = name;
    loop {
        if let Some(&(minimum, maximum)) = data.get(parent) {
            return minimum <= target && maximum.is_none_or(|maximum| target <= maximum);
        }
        let Some((prefix, _)) = parent.rsplit_once('.') else {
            return true;
        };
        parent = prefix;
    }
}
