//! Label the model a replay ran on: its GGUF architecture, and whether llama.cpp treats
//! that architecture as hybrid. The hybrid list has one owner, `llm_arch_is_hybrid` in the
//! vendored `src/llama-arch.cpp`; it is read from there, never copied here.

use std::io::Read;
use std::path::Path;

use crate::report::ModelLabel;

pub fn label(model: &Path, llama_src: &Path) -> ModelLabel {
    let arch = gguf_architecture(model).ok();
    let hybrid = match (&arch, std::fs::read_to_string(llama_src.join("src/llama-arch.cpp"))) {
        (Some(arch), Ok(source)) => hybrid_arches(&source).map(|list| list.iter().any(|a| a == arch)),
        _ => None,
    };
    ModelLabel { path: model.display().to_string(), arch, hybrid }
}

/// `general.architecture` from a GGUF header. GGUF writers put it first among the
/// key-values; the scan stops at the first non-string value rather than parse every type.
pub fn gguf_architecture(path: &Path) -> Result<String, String> {
    const GGUF_TYPE_STRING: u32 = 8;
    let mut f = std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if read_n::<4>(&mut f)? != *b"GGUF" {
        return Err(format!("{} is not GGUF", path.display()));
    }
    read_n::<4>(&mut f)?; // version
    read_n::<8>(&mut f)?; // tensor count
    let kv_count = u64::from_le_bytes(read_n::<8>(&mut f)?);
    for _ in 0..kv_count {
        let key = read_string(&mut f)?;
        if u32::from_le_bytes(read_n::<4>(&mut f)?) != GGUF_TYPE_STRING {
            break;
        }
        let value = read_string(&mut f)?;
        if key == "general.architecture" {
            return Ok(value);
        }
    }
    Err(format!("{} has no general.architecture before its first non-string key", path.display()))
}

fn read_n<const N: usize>(r: &mut impl Read) -> Result<[u8; N], String> {
    let mut buf = [0u8; N];
    r.read_exact(&mut buf).map_err(|e| format!("short GGUF header: {e}"))?;
    Ok(buf)
}

/// A GGUF string: u64 length, then UTF-8 bytes. Header strings are short; a length past
/// a MiB means the scan is no longer reading the header.
fn read_string(r: &mut impl Read) -> Result<String, String> {
    let len = u64::from_le_bytes(read_n::<8>(r)?);
    if len > 1 << 20 {
        return Err(format!("GGUF string of {len} bytes in the header"));
    }
    let mut bytes = vec![0u8; len as usize];
    r.read_exact(&mut bytes).map_err(|e| format!("short GGUF header: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("GGUF string is not UTF-8: {e}"))
}

/// The architecture names `llm_arch_is_hybrid` returns true for, from the C++ source:
/// the `LLM_ARCH_*` cases of that function, named through the `{ LLM_ARCH_X, "name" }` table.
pub fn hybrid_arches(source: &str) -> Option<Vec<String>> {
    let body_start = source.find("bool llm_arch_is_hybrid(")?;
    let body = &source[body_start..];
    let body = &body[..body.find("default:")?];
    let cases: Vec<&str> = body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("case ")?.strip_suffix(':'))
        .collect();
    let names = cases
        .iter()
        .filter_map(|case| {
            let entry = source.find(&format!("{{ {case},"))?;
            let rest = &source[entry..];
            let open = rest.find('"')? + 1;
            let close = open + rest[open..].find('"')?;
            Some(rest[open..close].to_owned())
        })
        .collect::<Vec<_>>();
    (names.len() == cases.len() && !names.is_empty()).then_some(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a model labelled with the wrong arch or hybrid status. The
    // report says DENSE or hybrid from these two reads; a parser that missed a case or
    // a table entry would label a hybrid run dense and hide that the checkpoint path,
    // the one SERVING-ENGINE.md says costs the most, was never exercised.
    #[test]
    fn arch_and_hybrid_list_are_read_from_their_owners() {
        let source = r#"
            static const std::map<llm_arch, const char *> LLM_ARCH_NAMES = {
                { LLM_ARCH_LLAMA,            "llama"            },
                { LLM_ARCH_QWEN35,           "qwen35"           },
                { LLM_ARCH_LFM2,             "lfm2"             },
            };
            bool llm_arch_is_hybrid(const llm_arch & arch) {
                switch (arch) {
                    case LLM_ARCH_LFM2:
                    case LLM_ARCH_QWEN35:
                        return true;
                    default:
                        return false;
                }
            }"#;
        assert_eq!(hybrid_arches(source), Some(vec!["lfm2".to_owned(), "qwen35".to_owned()]));
        assert_eq!(hybrid_arches("bool llm_arch_is_hybrid( case LLM_ARCH_X: default:"), None);

        let dir = std::env::temp_dir().join(format!("serving-replay-gguf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir"); // expect: a test without a temp dir cannot run
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(1u64.to_le_bytes());
        for s in ["general.architecture", "qwen35"] {
            if s == "qwen35" {
                bytes.extend(8u32.to_le_bytes());
            }
            bytes.extend((s.len() as u64).to_le_bytes());
            bytes.extend(s.as_bytes());
        }
        let path = dir.join("m.gguf");
        std::fs::write(&path, bytes).expect("fixture"); // expect: fixture write in the test's own temp dir
        let arch = gguf_architecture(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(arch, Ok("qwen35".to_owned()));
    }
}
