//! WAV File Loader for Test Audio
//!
//! Loads 16kHz mono WAV files for background noise testing.
//! Simple implementation for test purposes only.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Load a 16kHz mono WAV file and return PCM samples
///
/// # Arguments
/// * `path` - Path to WAV file
///
/// # Returns
/// * Vector of i16 PCM samples, or error if file cannot be read
pub fn load_wav_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<i16>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // Parse WAV header
    if &buffer[0..4] != b"RIFF" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a RIFF file"));
    }

    if &buffer[8..12] != b"WAVE" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a WAVE file"));
    }

    // Find data chunk
    let mut offset = 12;
    let data_offset = loop {
        if offset + 8 > buffer.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Data chunk not found"));
        }

        let chunk_id = &buffer[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            buffer[offset + 4],
            buffer[offset + 5],
            buffer[offset + 6],
            buffer[offset + 7],
        ]) as usize;

        if chunk_id == b"data" {
            break offset + 8;
        }

        offset += 8 + chunk_size;
    };

    // Read PCM data as i16 samples (little-endian)
    let data_end = buffer.len();
    let samples: Vec<i16> = buffer[data_offset..data_end]
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    Ok(samples)
}

/// Load background noise sample by name
///
/// Loads from test_audio/background_noise/ directory
pub fn load_background_noise(name: &str) -> io::Result<Vec<i16>> {
    let path = format!("test_audio/background_noise/{}.wav", name);
    load_wav_file(path)
}

/// Get a chunk of audio from a sample
///
/// Useful for getting exactly N samples from a longer WAV file
pub fn get_chunk(samples: &[i16], start: usize, length: usize) -> Vec<i16> {
    if start + length <= samples.len() {
        samples[start..start + length].to_vec()
    } else {
        // Loop if needed
        let mut result = Vec::with_capacity(length);
        let mut pos = start;

        for _ in 0..length {
            result.push(samples[pos % samples.len()]);
            pos += 1;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_background_noises() {
        // Test that all 10 generated noise files can be loaded
        let noises = vec![
            "01_white_noise",
            "02_pink_noise",
            "03_brown_noise",
            "04_hvac_hum",
            "05_fan_noise",
            "06_fluorescent_buzz",
            "07_office_ambiance",
            "08_crowd_murmur",
            "09_traffic_road",
            "10_restaurant_cafe",
        ];

        for noise in noises {
            match load_background_noise(noise) {
                Ok(samples) => {
                    println!("✓ Loaded {}: {} samples", noise, samples.len());
                    assert!(samples.len() > 0, "{} has no samples", noise);
                    // 5 seconds @ 16kHz = 80,000 samples
                    assert!(samples.len() >= 79000 && samples.len() <= 81000,
                        "{} has unexpected length: {}", noise, samples.len());
                }
                Err(e) => {
                    println!("✗ Failed to load {}: {}", noise, e);
                    println!("  Run: ./scripts/generate_10_noises.sh");
                    // Don't fail test - files may not exist in CI
                }
            }
        }
    }

    #[test]
    fn test_get_chunk() {
        let samples: Vec<i16> = (0..1000).map(|i| i as i16).collect();

        // Normal chunk
        let chunk = get_chunk(&samples, 100, 50);
        assert_eq!(chunk.len(), 50);
        assert_eq!(chunk[0], 100);
        assert_eq!(chunk[49], 149);

        // Chunk that loops
        let chunk = get_chunk(&samples, 990, 50);
        assert_eq!(chunk.len(), 50);
        assert_eq!(chunk[0], 990);
        assert_eq!(chunk[10], 0); // Looped back
    }
}
