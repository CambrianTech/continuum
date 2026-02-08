//! Phonemizer using espeak-ng for text-to-phoneme conversion
//! Piper TTS models require espeak-ng IPA phonemes

use std::collections::HashMap;
use std::process::Command;

pub struct Phonemizer {
    phoneme_to_id: HashMap<String, i64>,
}

impl Phonemizer {
    /// Load phoneme_id_map from Piper model config
    pub fn load_from_config(config_path: &str) -> Result<Self, String> {
        let config_content = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read model config: {e}"))?;

        let config: serde_json::Value = serde_json::from_str(&config_content)
            .map_err(|e| format!("Failed to parse model config: {e}"))?;

        let phoneme_id_map = config
            .get("phoneme_id_map")
            .ok_or("Missing phoneme_id_map in config")?;

        let mut phoneme_to_id = HashMap::new();

        if let Some(obj) = phoneme_id_map.as_object() {
            for (phoneme, ids) in obj {
                if let Some(id_array) = ids.as_array() {
                    if let Some(id) = id_array.first().and_then(|v| v.as_i64()) {
                        phoneme_to_id.insert(phoneme.clone(), id);
                    }
                }
            }
        }

        Ok(Self { phoneme_to_id })
    }

    /// Call espeak-ng to phonemize text
    fn call_espeak(&self, text: &str) -> Result<String, String> {
        let output = Command::new("/opt/homebrew/bin/espeak-ng")
            .args(["-v", "en-us", "-q", "--ipa=3"])
            .arg(text)
            .output()
            .map_err(|e| format!("Failed to run espeak-ng: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("espeak-ng failed: {stderr}"));
        }

        let phonemes = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string()
            // Remove zero-width joiners and other invisible characters
            .replace(['\u{200D}', '\u{200C}', '\u{FEFF}'], "")
            // Replace newlines with spaces (espeak-ng outputs multiple lines for punctuation)
            .replace(['\n', '\r'], " ");

        Ok(phonemes)
    }

    /// Convert text to phoneme IDs using espeak-ng
    pub fn text_to_phoneme_ids(&self, text: &str) -> Vec<i64> {
        // Get IPA phonemes from espeak-ng
        let phonemes = match self.call_espeak(text) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Phonemizer error: {e}");
                // Return minimal valid sequence on error
                return vec![1, 59, 2]; // ^, ə, $
            }
        };

        let mut ids = vec![1]; // BOS (beginning of sentence) = ^
        let mut unknown_count = 0;
        const PAD_ID: i64 = 0; // PAD token = _

        // Process each character in the IPA string
        for ch in phonemes.chars() {
            // Skip whitespace and control characters except space
            if ch.is_whitespace() && ch != ' ' {
                continue;
            }

            let ch_str = ch.to_string();

            if let Some(&id) = self.phoneme_to_id.get(&ch_str) {
                ids.push(id);
                ids.push(PAD_ID); // Add PAD after each phoneme
            } else {
                // Unknown phoneme - skip it
                unknown_count += 1;
                if unknown_count <= 5 {  // Only log first 5 to avoid spam
                    let ch_code = ch as u32;
                    eprintln!("Unknown phoneme '{ch}' (U+{ch_code:04X}), skipping");
                }
            }
        }

        if unknown_count > 5 {
            let remaining = unknown_count - 5;
            eprintln!("... and {remaining} more unknown phonemes");
        }

        // If we got no valid phonemes, return minimal sequence
        if ids.len() == 1 {
            if let Some(&schwa_id) = self.phoneme_to_id.get("ə") {
                ids.push(schwa_id);
                ids.push(PAD_ID);
            }
        }

        ids.push(2); // EOS (end of sentence) = $
        ids
    }
}

impl Default for Phonemizer {
    fn default() -> Self {
        // Load from default model config
        Self::load_from_config("../models/piper/en_US-libritts_r-medium.onnx.json")
            .unwrap_or_else(|e| {
                eprintln!("Failed to load phoneme map from config: {e}");
                Self { phoneme_to_id: HashMap::new() }
            })
    }
}
