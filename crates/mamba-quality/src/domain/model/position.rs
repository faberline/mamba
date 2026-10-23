//! Source lines keep unsigned API values and the original absent-position marker.
use super::QualityLocation;
use serde::{de::Error, Deserialize, Deserializer, Serializer};

fn encode(line: u32) -> i64 {
    if line == QualityLocation::UNPOSITIONED_LINE {
        -1
    } else {
        i64::from(line)
    }
}

fn decode<E: Error>(line: i64) -> Result<u32, E> {
    if line == -1 {
        Ok(QualityLocation::UNPOSITIONED_LINE)
    } else {
        u32::try_from(line).map_err(|_| E::custom("source line must be -1 or an unsigned integer"))
    }
}

pub(super) fn serialize<S: Serializer>(line: &u32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_i64(encode(*line))
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    decode(i64::deserialize(deserializer)?)
}

pub(super) fn serialize_optional<S: Serializer>(
    line: &Option<u32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match line {
        Some(line) => serializer.serialize_some(&encode(*line)),
        None => serializer.serialize_none(),
    }
}

pub(super) fn deserialize_optional<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    Option::<i64>::deserialize(deserializer)?
        .map(decode)
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_positions_preserve_original_absent_and_ordinary_lines() {
        for line in [-1, 0, 1, 312, 4294967294_i64] {
            let value = json!({"line":line,"column":0,"end_line":line,"end_column":0,"offset":null,"end_offset":null});
            let location: QualityLocation = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&location).unwrap(), value);
            if line == -1 {
                assert_eq!(location, QualityLocation::unpositioned());
            }
        }
        for line in [-2, -100, 4294967296_i64] {
            let value = json!({"line":line,"column":0,"end_line":null,"end_column":null,"offset":null,"end_offset":null});
            assert!(serde_json::from_value::<QualityLocation>(value).is_err());
        }
        let value = json!({"line":1,"column":1,"end_line":null,"end_column":null,"offset":null,"end_offset":null});
        let location: QualityLocation = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(location).unwrap(), value);

        let value = json!({"line":5,"column":0,"end_line":5,"end_column":-1,"offset":null,"end_offset":null});
        let location: QualityLocation = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(location.end_column, Some(-1));
        assert_eq!(serde_json::to_value(location).unwrap(), value);
    }
}
