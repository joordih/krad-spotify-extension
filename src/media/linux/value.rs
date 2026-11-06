use zbus::zvariant::Value;

const LIST_SEPARATOR: &str = ", ";

pub(super) fn text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.as_str().to_owned()),
        Value::ObjectPath(path) => Some(path.as_str().to_owned()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(text).collect();
            (!parts.is_empty()).then(|| parts.join(LIST_SEPARATOR))
        }
        Value::Value(inner) => text(inner),
        _ => None,
    }
}

pub(super) fn boolean(value: &Value<'_>) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Value(inner) => boolean(inner),
        _ => None,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a finite float is truncated toward zero, saturating at the i64 bounds"
)]
pub(super) fn integer(value: &Value<'_>) -> Option<i64> {
    match value {
        Value::I64(number) => Some(*number),
        Value::U64(number) => i64::try_from(*number).ok(),
        Value::I32(number) => Some(i64::from(*number)),
        Value::U32(number) => Some(i64::from(*number)),
        Value::F64(number) if number.is_finite() => Some(*number as i64),
        Value::Value(inner) => integer(inner),
        _ => None,
    }
}

pub(super) fn float(value: &Value<'_>) -> Option<f64> {
    match value {
        Value::F64(number) if number.is_finite() => Some(*number),
        Value::Value(inner) => float(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_in_any_integer_or_text_form() {
        assert_eq!(integer(&Value::U64(180_000_000)), Some(180_000_000));
        assert_eq!(integer(&Value::I32(3)), Some(3));
        assert_eq!(text(&Value::from("Song")).as_deref(), Some("Song"));
        assert_eq!(text(&Value::from(vec!["A", "B"])).as_deref(), Some("A, B"));
    }
}
