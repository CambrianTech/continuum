//! Inspect catalog syntax tokens, never substrings inside names or quoted values.
//! This is a conservative proof guard, not a general SQL equivalence parser.
pub(super) fn requires_expression_proof(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' | b'"' | b'`' | b'[' => {
                let close = if bytes[at] == b'[' { b']' } else { bytes[at] };
                at += 1;
                loop {
                    if at == bytes.len() { return true; }
                    if bytes[at] == close {
                        at += 1;
                        if close != b']' && bytes.get(at) == Some(&close) { at += 1; }
                        else { break; }
                    } else { at += 1; }
                }
            }
            b'-' if bytes.get(at + 1) == Some(&b'-') => {
                while at < bytes.len() && bytes[at] != b'\n' { at += 1; }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                while at + 1 < bytes.len() && &bytes[at..at + 2] != b"*/" { at += 1; }
                if at + 1 >= bytes.len() { return true; }
                at += 2;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = at;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') { at += 1; }
                let token = &bytes[start..at];
                if [b"CHECK".as_slice(), b"WHERE", b"GENERATED", b"COLLATE", b"WITHOUT", b"STRICT",
                    b"AUTOINCREMENT", b"CONFLICT", b"DEFERRABLE", b"INITIALLY", b"VIRTUAL"]
                    .iter().any(|keyword| token.eq_ignore_ascii_case(keyword)) { return true; }
            }
            _ => at += 1,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::requires_expression_proof as guarded;
    #[test]
    fn syntax_keywords_are_distinct_from_names_and_fk_actions() {
        assert!(!guarded("CREATE TABLE training_trigger_learn_checkpoints (id TEXT PRIMARY KEY, child TEXT REFERENCES parent(id) ON UPDATE RESTRICT)"));
        assert!(!guarded("CREATE TABLE \"strict\" ([check] TEXT DEFAULT 'it''s CHECK', `where` TEXT /* CHECK */) -- STRICT"));
        for sql in ["CREATE TABLE x (a TEXT CHECK(length(a)>0))", "CREATE INDEX x ON t(a) WHERE a IS NOT NULL",
            "CREATE TABLE x (a TEXT COLLATE NOCASE)", "CREATE TABLE x (a INT) STRICT", "CREATE TABLE x (a TEXT DEFAULT 'unterminated"] {
            assert!(guarded(sql), "{sql}");
        }
    }
}
