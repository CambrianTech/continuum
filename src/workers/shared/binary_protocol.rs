/// Binary Data Protocol - Zero-Copy IPC for Large Payloads
///
/// PHILOSOPHY: JSON for control, binary for data.
///
/// Control messages (ping, status, commands) stay JSON - they're small and text-based.
/// Data payloads (embeddings, frames, audio, tensors) go binary - NO SERIALIZATION.
///
/// Wire format for binary responses:
/// ```
/// | JSON header (newline-terminated) | Binary payload |
/// | {"type":"binary","length":1536,"dtype":"f32","shape":[384]} \n | raw bytes... |
/// ```
///
/// Why this format?
/// 1. Header is still JSON - easy parsing, self-describing
/// 2. Payload is raw bytes - zero copy, zero parse
/// 3. Length in header - receiver knows exactly how many bytes to read
/// 4. dtype/shape - receiver can interpret bytes correctly
///
/// REUSABLE FOR:
/// - Embeddings (f32 vectors)
/// - Video frames (u8 RGB/YUV buffers)
/// - Audio samples (f32 or i16 PCM)
/// - Model weights (f16/bf16 tensors)
/// - Screenshots (PNG bytes)

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

// ============================================================================
// Binary Header (JSON portion)
// ============================================================================

/// Header for binary payload - sent as JSON, terminated by newline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryHeader {
    /// Always "binary" for binary payloads
    #[serde(rename = "type")]
    pub r#type: String,

    /// Total payload length in bytes
    pub length: usize,

    /// Data type (f32, f16, u8, i16, etc.)
    pub dtype: DataType,

    /// Shape of the data (e.g., [384] for embedding, [1920, 1080, 3] for frame)
    pub shape: Vec<usize>,

    /// Number of items if this is a batch (e.g., 10 embeddings)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,

    /// Optional metadata (model name, timestamp, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Data types for binary payloads
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    /// 32-bit float (embeddings, audio)
    F32,
    /// 16-bit float (half precision weights)
    F16,
    /// 8-bit unsigned (images, raw bytes)
    U8,
    /// 16-bit signed (audio samples)
    I16,
    /// Brain float 16 (ML tensors)
    Bf16,
    /// 64-bit float (high precision)
    F64,
}

impl DataType {
    /// Size in bytes per element
    pub fn size_bytes(&self) -> usize {
        match self {
            DataType::F32 => 4,
            DataType::F16 => 2,
            DataType::U8 => 1,
            DataType::I16 => 2,
            DataType::Bf16 => 2,
            DataType::F64 => 8,
        }
    }
}

// ============================================================================
// Binary Writer - Rust side (send binary payload)
// ============================================================================

/// Write a binary payload to a stream
///
/// Format: JSON header (newline-terminated) + raw bytes
pub fn write_binary_payload<W: Write>(
    writer: &mut W,
    data: &[u8],
    dtype: DataType,
    shape: Vec<usize>,
    batch_size: Option<usize>,
    metadata: Option<serde_json::Value>,
) -> std::io::Result<()> {
    let header = BinaryHeader {
        r#type: "binary".to_string(),
        length: data.len(),
        dtype,
        shape,
        batch_size,
        metadata,
    };

    // Write JSON header with newline terminator
    let header_json = serde_json::to_string(&header)?;
    writer.write_all(header_json.as_bytes())?;
    writer.write_all(b"\n")?;

    // Write raw binary payload - NO SERIALIZATION
    writer.write_all(data)?;
    writer.flush()?;

    Ok(())
}

/// Convenience: Write f32 embeddings as binary
pub fn write_embeddings<W: Write>(
    writer: &mut W,
    embeddings: &[Vec<f32>],
    model: Option<String>,
) -> std::io::Result<()> {
    if embeddings.is_empty() {
        return write_binary_payload(writer, &[], DataType::F32, vec![0], Some(0), None);
    }

    let dims = embeddings[0].len();
    let batch_size = embeddings.len();

    // Flatten to contiguous bytes - this is the ONLY copy we make
    // Using a flat buffer with known capacity avoids reallocation
    let total_floats = batch_size * dims;
    let mut flat: Vec<f32> = Vec::with_capacity(total_floats);
    for emb in embeddings {
        flat.extend_from_slice(emb);
    }

    // Cast to bytes - ZERO COPY via bytemuck-style reinterpret
    // Safety: f32 is Pod (plain old data), any bit pattern is valid
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(flat.as_ptr() as *const u8, flat.len() * 4)
    };

    let metadata = model.map(|m| serde_json::json!({ "model": m }));

    write_binary_payload(
        writer,
        bytes,
        DataType::F32,
        vec![dims],
        Some(batch_size),
        metadata,
    )
}

/// Convenience: Write a single f32 embedding
pub fn write_single_embedding<W: Write>(
    writer: &mut W,
    embedding: &[f32],
    model: Option<String>,
) -> std::io::Result<()> {
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(embedding.as_ptr() as *const u8, embedding.len() * 4)
    };

    let metadata = model.map(|m| serde_json::json!({ "model": m }));

    write_binary_payload(
        writer,
        bytes,
        DataType::F32,
        vec![embedding.len()],
        Some(1),
        metadata,
    )
}

// ============================================================================
// Binary Reader - Rust side (receive binary payload)
// ============================================================================

/// Read a binary header from a stream (reads until newline)
pub fn read_binary_header<R: Read>(reader: &mut R) -> std::io::Result<BinaryHeader> {
    let mut header_bytes = Vec::with_capacity(512);
    let mut byte = [0u8; 1];

    // Read until newline
    loop {
        reader.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        header_bytes.push(byte[0]);
    }

    let header_str = String::from_utf8(header_bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    serde_json::from_str(&header_str)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Read binary payload into a pre-allocated buffer
pub fn read_binary_payload<R: Read>(
    reader: &mut R,
    header: &BinaryHeader,
    buffer: &mut [u8],
) -> std::io::Result<()> {
    if buffer.len() < header.length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Buffer too small: need {} bytes, got {}",
                header.length,
                buffer.len()
            ),
        ));
    }

    reader.read_exact(&mut buffer[..header.length])
}

/// Read f32 embeddings from binary payload
pub fn read_embeddings<R: Read>(reader: &mut R) -> std::io::Result<Vec<Vec<f32>>> {
    let header = read_binary_header(reader)?;

    if header.dtype != DataType::F32 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Expected f32, got {:?}", header.dtype),
        ));
    }

    let dims = header.shape.first().copied().unwrap_or(0);
    let batch_size = header.batch_size.unwrap_or(1);

    // Allocate buffer for raw bytes
    let mut buffer = vec![0u8; header.length];
    reader.read_exact(&mut buffer)?;

    // Reinterpret as f32 - ZERO COPY
    let floats: &[f32] = unsafe {
        std::slice::from_raw_parts(buffer.as_ptr() as *const f32, buffer.len() / 4)
    };

    // Split into individual embeddings
    let mut embeddings = Vec::with_capacity(batch_size);
    for i in 0..batch_size {
        let start = i * dims;
        let end = start + dims;
        embeddings.push(floats[start..end].to_vec());
    }

    Ok(embeddings)
}

// ============================================================================
// Video/Frame Support (Future)
// ============================================================================

/// Frame format for video data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FrameFormat {
    /// RGB 8-bit per channel
    Rgb8,
    /// RGBA 8-bit per channel
    Rgba8,
    /// YUV 4:2:0 (common video format)
    Yuv420,
    /// Grayscale 8-bit
    Gray8,
    /// Grayscale 32-bit float
    Gray32f,
}

/// Write a video frame as binary
pub fn write_frame<W: Write>(
    writer: &mut W,
    pixels: &[u8],
    width: usize,
    height: usize,
    format: FrameFormat,
) -> std::io::Result<()> {
    let metadata = serde_json::json!({
        "width": width,
        "height": height,
        "format": format,
    });

    write_binary_payload(
        writer,
        pixels,
        DataType::U8,
        vec![height, width, 3], // HWC format
        None,
        Some(metadata),
    )
}

// ============================================================================
// Response Type Detection
// ============================================================================

/// Peek at the first character to determine if response is JSON or binary
///
/// Returns:
/// - true if first byte indicates JSON ('{' or '[')
/// - false if first byte indicates binary header
pub fn is_json_response(first_byte: u8) -> bool {
    first_byte == b'{' || first_byte == b'['
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_write_read_embeddings() {
        let embeddings = vec![
            vec![1.0f32, 2.0, 3.0, 4.0],
            vec![5.0f32, 6.0, 7.0, 8.0],
        ];

        let mut buffer = Vec::new();
        write_embeddings(&mut buffer, &embeddings, Some("test-model".to_string())).unwrap();

        let mut cursor = Cursor::new(buffer);
        let read_embeddings = read_embeddings(&mut cursor).unwrap();

        assert_eq!(read_embeddings.len(), 2);
        assert_eq!(read_embeddings[0], vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(read_embeddings[1], vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn test_binary_size_vs_json() {
        // 384-dim embedding
        let embedding: Vec<f32> = (0..384).map(|i| i as f32 * 0.01).collect();

        // Binary size
        let mut binary_buf = Vec::new();
        write_single_embedding(&mut binary_buf, &embedding, None).unwrap();
        let binary_size = binary_buf.len();

        // JSON size
        let json_str = serde_json::to_string(&embedding).unwrap();
        let json_size = json_str.len();

        println!("Binary: {} bytes", binary_size);
        println!("JSON: {} bytes", json_size);
        println!("Ratio: {:.2}x", json_size as f64 / binary_size as f64);

        // Binary should be ~3x smaller
        assert!(binary_size < json_size);
    }
}
