//! Percent conversion scanning counts Python character positions and star operands.
pub(in crate::infrastructure::pylint) struct Format {
    pub arguments: usize,
    pub mapping: bool,
    pub keys: Vec<(String, char)>,
    pub conversions: Vec<char>,
}

pub(in crate::infrastructure::pylint) enum Error {
    Truncated,
    Unsupported(char, usize),
}

pub(in crate::infrastructure::pylint) fn parse(text: &str) -> Result<Format, Error> {
    let values: Vec<_> = text.chars().collect();
    let mut result = Format {
        arguments: 0,
        mapping: false,
        keys: Vec::new(),
        conversions: Vec::new(),
    };
    let mut position = 0;
    while position < values.len() {
        if values[position] != '%' {
            position += 1;
            continue;
        }
        position += 1;
        let mut key = None;
        if values.get(position) == Some(&'(') {
            position += 1;
            let start = position;
            let mut nesting = 1;
            while nesting != 0 {
                let value = *values.get(position).ok_or(Error::Truncated)?;
                nesting += usize::from(value == '(');
                nesting -= usize::from(value == ')');
                position += 1;
            }
            if position > start + 1 {
                key = Some(values[start..position - 1].iter().collect::<String>());
            }
        }
        while values
            .get(position)
            .is_some_and(|value| "#0- +".contains(*value))
        {
            position += 1;
        }
        if values.get(position) == Some(&'*') {
            result.arguments += 1;
            position += 1;
        } else {
            while values.get(position).is_some_and(char::is_ascii_digit) {
                position += 1;
            }
        }
        if values.get(position) == Some(&'.') {
            position += 1;
            if values.get(position) == Some(&'*') {
                result.arguments += 1;
                position += 1;
            } else {
                while values.get(position).is_some_and(char::is_ascii_digit) {
                    position += 1;
                }
            }
        }
        if values
            .get(position)
            .is_some_and(|value| "hlL".contains(*value))
        {
            position += 1;
        }
        let character = *values.get(position).ok_or(Error::Truncated)?;
        if !"diouxXeEfFgGcrs%a".contains(character) {
            return Err(Error::Unsupported(character, position));
        }
        if let Some(key) = key {
            result.mapping = true;
            if let Some((_, conversion)) = result.keys.iter_mut().find(|(name, _)| *name == key) {
                *conversion = character;
            } else {
                result.keys.push((key, character));
            }
        } else if character != '%' {
            result.arguments += 1;
            result.conversions.push(character);
        }
        position += 1;
    }
    Ok(result)
}

pub(in crate::infrastructure::pylint) fn repr(character: char) -> String {
    match character {
        '\'' => "\"'\"".into(),
        '\\' => "'\\\\'".into(),
        '\n' => "'\\n'".into(),
        '\r' => "'\\r'".into(),
        '\t' => "'\\t'".into(),
        value if value.is_control() && u32::from(value) < 256 => {
            format!("'\\x{:02x}'", u32::from(value))
        }
        value => format!("'{value}'"),
    }
}
