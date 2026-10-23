//! PEP 3101 parsing retains nested fields and the original distinct-index count.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    Index(usize),
    Name(String),
}

pub(super) struct Field {
    pub key: Key,
    pub access: Vec<(bool, Key)>,
}

#[derive(Default)]
pub(super) struct Format {
    pub fields: Vec<Field>,
    pub implicit: usize,
    pub manual: usize,
    pub complex: bool,
}

fn key(text: &str) -> Key {
    text.parse()
        .map(Key::Index)
        .unwrap_or_else(|_| Key::Name(text.to_string()))
}

fn field(text: &str) -> Result<Field, ()> {
    let chars: Vec<_> = text.chars().collect();
    let mut position = chars
        .iter()
        .position(|c| matches!(c, '.' | '['))
        .unwrap_or(chars.len());
    let root: String = chars[..position].iter().collect();
    let mut result = Field {
        key: key(&root),
        access: Vec::new(),
    };
    while position < chars.len() {
        let attribute = chars[position] == '.';
        if !attribute && chars[position] != '[' {
            return Err(());
        }
        position += 1;
        let start = position;
        if attribute {
            while chars.get(position).is_some_and(|c| !matches!(c, '.' | '[')) {
                position += 1;
            }
        } else {
            while chars.get(position).is_some_and(|&c| c != ']') {
                position += 1;
            }
            if chars.get(position) != Some(&']') {
                return Err(());
            }
        }
        if position == start {
            return Err(());
        }
        let text: String = chars[start..position].iter().collect();
        result.access.push((
            attribute,
            if attribute {
                Key::Name(text)
            } else {
                key(&text)
            },
        ));
        if !attribute {
            position += 1;
        }
    }
    Ok(result)
}

fn collect(text: &str, names: &mut Vec<String>, complex: &mut bool) -> Result<(), ()> {
    let chars: Vec<_> = text.chars().collect();
    let mut position = 0;
    while position < chars.len() {
        let c = chars[position];
        if matches!(c, '{' | '}') && chars.get(position + 1) == Some(&c) {
            position += 2;
            continue;
        }
        if c == '}' {
            return Err(());
        }
        if c != '{' {
            position += 1;
            continue;
        }
        position += 1;
        let start = position;
        while let Some(&c) = chars.get(position) {
            if matches!(c, '}' | '!' | ':') {
                break;
            }
            if c == '{' {
                return Err(());
            }
            if c == '[' {
                position += 1;
                while chars.get(position).is_some_and(|&c| c != ']') {
                    position += 1;
                }
                if chars.get(position) != Some(&']') {
                    return Err(());
                }
            }
            position += 1;
        }
        names.push(chars[start..position].iter().collect());
        if chars.get(position) == Some(&'!') {
            position += 2;
            if !chars.get(position).is_some_and(|c| matches!(c, ':' | '}')) {
                return Err(());
            }
        }
        if chars.get(position) == Some(&':') {
            position += 1;
            let start = position;
            let mut depth = 1;
            while depth != 0 {
                let c = *chars.get(position).ok_or(())?;
                depth += usize::from(c == '{');
                depth -= usize::from(c == '}');
                position += 1;
            }
            let specification: String = chars[start..position - 1].iter().collect();
            *complex |= !specification.is_empty();
            collect(&specification, names, complex)?;
        } else {
            if chars.get(position) != Some(&'}') {
                return Err(());
            }
            position += 1;
        }
    }
    Ok(())
}

pub(in crate::infrastructure::pylint) fn parse(text: &str) -> Result<Format, ()> {
    let mut result = Format::default();
    let mut names = Vec::new();
    let mut manual = std::collections::BTreeSet::new();
    collect(text, &mut names, &mut result.complex)?;
    for name in names {
        if name.is_empty() {
            result.implicit += 1;
        } else if let Ok(index) = name.parse::<usize>() {
            manual.insert(index);
        } else {
            let field = field(&name)?;
            if let Key::Index(index) = field.key {
                manual.insert(index);
            }
            result.fields.push(field);
        }
    }
    result.manual = manual.len();
    Ok(result)
}
