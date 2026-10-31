//! RFC 8785 JSON Canonicalization Scheme (JCS): the one bytes rule for a manifest and for an
//! alloy's signed payload (forge-alloy signs JCS too), so a package id and a provenance
//! signature are computed over the same canonical form and one verifier serves both.
//!
//! - object members sorted by their names' UTF-16 code units, at every depth;
//! - numbers as ECMAScript `Number.prototype.toString` renders the IEEE-754 double;
//! - strings with only `"` `\` and the C0 controls escaped (`\b \f \n \r \t`, else `\u00xx`);
//! - no insignificant whitespace.
//!
//! An integer beyond ±2^53 has no exact double, so it is refused rather than rounded.

use serde_json::{Number, Value};

/// The largest integer every JCS implementation holds exactly (2^53).
const SAFE_INTEGER: u64 = 1 << 53;

/// A value with no canonical form.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("integer {0} is outside ±2^53; RFC 8785 numbers are IEEE-754 doubles")]
    UnsafeInteger(Number),
    #[error("number {0} has no finite double")]
    NotFinite(Number),
}

/// The RFC 8785 bytes of `value`.
pub fn canonicalize(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::with_capacity(256);
    write(&mut out, value)?;
    Ok(out)
}

fn write(out: &mut Vec<u8>, value: &Value) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => write_number(out, n)?,
        Value::String(s) => write_str(out, s),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write(out, item)?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push(b'{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_str(out, k);
                out.push(b':');
                write(out, v)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn write_number(out: &mut Vec<u8>, n: &Number) -> Result<(), CanonicalError> {
    if let Some(u) = n.as_u64() {
        return if u <= SAFE_INTEGER {
            out.extend_from_slice(u.to_string().as_bytes());
            Ok(())
        } else {
            Err(CanonicalError::UnsafeInteger(n.clone()))
        };
    }
    if let Some(i) = n.as_i64() {
        return if i.unsigned_abs() <= SAFE_INTEGER {
            out.extend_from_slice(i.to_string().as_bytes());
            Ok(())
        } else {
            Err(CanonicalError::UnsafeInteger(n.clone()))
        };
    }
    match n.as_f64() {
        Some(x) if x.is_finite() => write_double(out, x, n),
        _ => Err(CanonicalError::NotFinite(n.clone())),
    }
}

/// ECMAScript Number::toString(x) for a finite double.
fn write_double(out: &mut Vec<u8>, x: f64, num: &Number) -> Result<(), CanonicalError> {
    if x == 0.0 {
        out.push(b'0'); // both zeros
        return Ok(());
    }
    if x < 0.0 {
        out.push(b'-');
    }
    // serde_json renders a double with ryu: the shortest digits that round-trip, the closest
    // on a tie, which are the digits ECMAScript chooses. Only the layout differs ("1e23",
    // "0.001", "100.0"), so take digits and decimal point from it and lay them out per ES.
    let text = num.to_string();
    let not_finite = || CanonicalError::NotFinite(num.clone());
    let unsigned = text.trim_start_matches('-');
    let (mantissa, exp) = match unsigned.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().map_err(|_| not_finite())?),
        None => (unsigned, 0),
    };
    let (whole, frac) = match mantissa.split_once('.') {
        Some(parts) => parts,
        None => (mantissa, ""),
    };
    let all: Vec<u8> = whole.bytes().chain(frac.bytes()).collect();
    let lead = all.iter().take_while(|b| **b == b'0').count();
    let digits = &all[lead..];
    let digits = &digits[..digits.len() - digits.iter().rev().take_while(|b| **b == b'0').count()];
    if digits.is_empty() {
        return Err(not_finite());
    }
    let k = digits.len() as i32;
    // value = 0.d1d2..dk × 10^n
    let n = whole.len() as i32 + exp - lead as i32;
    if k <= n && n <= 21 {
        out.extend_from_slice(digits);
        out.extend(std::iter::repeat_n(b'0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        out.extend_from_slice(int);
        out.push(b'.');
        out.extend_from_slice(frac);
    } else if -6 < n && n <= 0 {
        out.extend_from_slice(b"0.");
        out.extend(std::iter::repeat_n(b'0', (-n) as usize));
        out.extend_from_slice(digits);
    } else {
        let e = n - 1;
        let (first, rest) = digits.split_at(1);
        out.extend_from_slice(first);
        if !rest.is_empty() {
            out.push(b'.');
            out.extend_from_slice(rest);
        }
        out.push(b'e');
        out.push(if e >= 0 { b'+' } else { b'-' });
        out.extend_from_slice(e.unsigned_abs().to_string().as_bytes());
    }
    Ok(())
}

/// JCS string escaping: `"` `\` and C0 controls; everything else verbatim UTF-8.
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

    fn jcs(v: &Value) -> String {
        String::from_utf8(canonicalize(v).unwrap()).unwrap()
    }

    // what this catches: any drift from ECMAScript number rendering, using every finite
    // vector of RFC 8785 Appendix B (IEEE-754 bits -> expected text): exponent thresholds at
    // 1e21 and 1e-7, the shortest-digits choice on near-ties, subnormals, the extremes, -0.
    #[test]
    fn rfc8785_appendix_b_numbers() {
        let vectors: &[(u64, &str)] = &[
            (0x0000000000000000, "0"),
            (0x8000000000000000, "0"),
            (0x0000000000000001, "5e-324"),
            (0x8000000000000001, "-5e-324"),
            (0x7fefffffffffffff, "1.7976931348623157e+308"),
            (0xffefffffffffffff, "-1.7976931348623157e+308"),
            (0x4340000000000000, "9007199254740992"),
            (0xc340000000000000, "-9007199254740992"),
            (0x4430000000000000, "295147905179352830000"),
            (0x44b52d02c7e14af5, "9.999999999999997e+22"),
            (0x44b52d02c7e14af6, "1e+23"),
            (0x44b52d02c7e14af7, "1.0000000000000001e+23"),
            (0x444b1ae4d6e2ef4e, "999999999999999700000"),
            (0x444b1ae4d6e2ef4f, "999999999999999900000"),
            (0x444b1ae4d6e2ef50, "1e+21"),
            (0x3eb0c6f7a0b5ed8c, "9.999999999999997e-7"),
            (0x3eb0c6f7a0b5ed8d, "0.000001"),
            (0x41b3de4355555553, "333333333.3333332"),
            (0x41b3de4355555554, "333333333.33333325"),
            (0x41b3de4355555555, "333333333.3333333"),
            (0x41b3de4355555556, "333333333.3333334"),
            (0x41b3de4355555557, "333333333.33333343"),
            (0xbecbf647612f3696, "-0.0000033333333333333333"),
            (0x43143ff3c1cb0959, "1424953923781206.2"),
        ];
        for &(bits, want) in vectors {
            let v = Value::from(f64::from_bits(bits));
            assert_eq!(jcs(&v), want, "bits {bits:016x}");
        }
    }

    // what this catches: sorting by UTF-8 bytes or code points instead of UTF-16 code units
    // (RFC 8785 §3.2.3: the emoji U+1F600 is the surrogate pair D83D DE00, so it sorts BEFORE
    // U+FB33, where byte order puts it after).
    #[test]
    fn rfc8785_sorts_names_by_utf16_code_units() {
        let v: Value = serde_json::from_str(
            r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#,
        )
        .unwrap();
        assert_eq!(
            jcs(&v),
            "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",\"\u{f6}\":\"Latin Small Letter O With Diaeresis\",\"\u{20ac}\":\"Euro Sign\",\"\u{1f600}\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}"
        );
    }

    // what this catches: the RFC 8785 §3.2.2 example end to end (parse, then canonicalize):
    // number re-rendering (4.50 -> 4.5, 1E30 -> 1e+30), string escapes (\u000F -> \u000f,
    // \/ -> /), literals and member order. It also proves serde_json parses the hard decimal
    // 333333333.33333329 to the nearest double (the `float_roundtrip` feature).
    #[test]
    fn rfc8785_section_3_2_2_example() {
        let v: Value = serde_json::from_str(
            r#"{"numbers":[333333333.33333329,1E30,4.50,2e-3,0.000000000000000000000000001],"string":"\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/","literals":[null,true,false]}"#,
        )
        .unwrap();
        assert_eq!(
            jcs(&v),
            "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}"
        );
    }

    // what this catches: map order never reaches the bytes, and canonical bytes re-parse and
    // re-encode to themselves (a manifest's id is stable across a load/save round trip).
    #[test]
    fn source_order_never_reaches_the_bytes() {
        let a: Value =
            serde_json::from_str(r#"{"b":1,"a":{"z":[true,null],"y":"q\"\n\u0001é"}}"#).unwrap();
        let b = json!({"a": {"y": "q\"\n\u{1}é", "z": [true, null]}, "b": 1});
        let bytes = canonicalize(&a).unwrap();
        assert_eq!(bytes, canonicalize(&b).unwrap());
        assert_eq!(bytes, r#"{"a":{"y":"q\"\n\u0001é","z":[true,null]},"b":1}"#.as_bytes());
        assert_eq!(canonicalize(&serde_json::from_slice(&bytes).unwrap()).unwrap(), bytes);
    }

    // what this catches: an integer no double holds exactly is refused, never silently rounded
    // into an id a JavaScript or Python verifier would compute differently; 2^53 itself passes.
    #[test]
    fn integers_beyond_2_pow_53_are_refused() {
        assert_eq!(jcs(&json!(9_007_199_254_740_992u64)), "9007199254740992");
        assert_eq!(jcs(&json!(-9_007_199_254_740_992i64)), "-9007199254740992");
        assert!(matches!(
            canonicalize(&json!({"size": 9_007_199_254_740_993u64})),
            Err(CanonicalError::UnsafeInteger(_))
        ));
        assert!(matches!(
            canonicalize(&json!([i64::MIN])),
            Err(CanonicalError::UnsafeInteger(_))
        ));
    }
}
