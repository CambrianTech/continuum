//! Canonical JSON: object keys sorted by byte order at every depth, no whitespace, one escape
//! spelling. The same value always encodes to the same bytes, so it always has the same id,
//! whatever map order serde_json was built with (its `preserve_order` feature is unified
//! across the workspace, so this layer never relies on the default).

use serde_json::Value;

/// The canonical bytes of `value`.
pub fn to_vec(value: &Value) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    write(&mut out, value);
    out
}

fn write(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => out.extend_from_slice(n.to_string().as_bytes()),
        Value::String(s) => write_str(out, s),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write(out, item);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push(b'{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_str(out, k);
                out.push(b':');
                write(out, v);
            }
            out.push(b'}');
        }
    }
}

/// serde_json's escape set: `"` `\` and control characters; everything else verbatim UTF-8.
fn write_str(out: &mut Vec<u8>, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(b'"');
    for &b in s.as_bytes() {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0c => out.extend_from_slice(b"\\f"),
            0x00..=0x1f => {
                out.extend_from_slice(b"\\u00");
                out.push(HEX[usize::from(b >> 4)]);
                out.push(HEX[usize::from(b & 0x0f)]);
            }
            _ => out.push(b),
        }
    }
    out.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: key order in the source map never reaches the bytes (sorted at every
    // depth), and the string escapes match serde_json's, so a canonical manifest re-parses to
    // the same value and re-encodes to the same bytes (the id is stable across a round trip).
    #[test]
    fn keys_sort_at_every_depth_and_strings_escape_like_serde_json() {
        let a: Value =
            serde_json::from_str(r#"{"b":1,"a":{"z":[true,null],"y":"q\"\n\u0001é"}}"#).unwrap();
        let b = json!({"a": {"y": "q\"\n\u{1}é", "z": [true, null]}, "b": 1});
        assert_eq!(to_vec(&a), to_vec(&b));
        let bytes = to_vec(&a);
        assert_eq!(
            bytes,
            r#"{"a":{"y":"q\"\n\u0001é","z":[true,null]},"b":1}"#.as_bytes()
        );
        assert_eq!(to_vec(&serde_json::from_slice(&bytes).unwrap()), bytes);
    }
}
