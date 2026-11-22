//! The capture's binary frame (card 699578cb; Joel, 2026-11-22: "binary streams and binary saves are
//! the default; JSON only for small, rare metadata"). A frame is one record: a small typed header and
//! the record's bulk as raw byte runs beside it.
//!
//! ```text
//! [u32 frame length][u8 kind][u32 header length][header][u16 run count]([u32 run length][run bytes])*
//! ```
//!
//! All integers are little-endian; the frame length counts everything after itself. The header is the
//! record's typed fields as a MessagePack array (`rmp_serde::to_vec`: positions, never field names, so
//! no row repeats its schema). The runs are the bulk (prompt messages, the raw stream, tool results)
//! copied straight in: no escaping, no base64, readable in place from a mapped file. A reader works
//! over a byte slice and refuses a short or inconsistent frame by name, never panics on it.

use serde::{de::DeserializeOwned, Serialize};

/// What a frame records; the reader dispatches on it before decoding the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// A call's request as it was submitted.
    Submitted = 1,
    /// A call's end: its response, error and raw stream.
    Terminal = 2,
    /// A record that could not be written whole, said in a small receipt.
    Incomplete = 3,
}

impl FrameKind {
    fn of(byte: u8) -> Result<Self, FrameError> {
        match byte {
            1 => Ok(Self::Submitted),
            2 => Ok(Self::Terminal),
            3 => Ok(Self::Incomplete),
            other => Err(FrameError::UnknownKind(other)),
        }
    }
}

/// Why bytes are not a frame, or a frame not a record.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("a frame needs {needed} more bytes than the {available} available: a truncated tail")]
    Short { needed: usize, available: usize },
    #[error("frame kind {0} is not one this capture writes")]
    UnknownKind(u8),
    #[error("a frame's parts ({parts} bytes) disagree with its stated length ({stated})")]
    Inconsistent { parts: usize, stated: usize },
    #[error("a frame part of {0} bytes does not fit the format's 32-bit length")]
    TooLong(usize),
    #[error("the frame holds {0} runs, over the format's 16-bit count")]
    TooManyRuns(usize),
    #[error("the header did not encode: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("the header did not decode: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// One frame read in place: its kind, its header bytes and its runs, all borrowing the source slice.
#[derive(Debug)]
pub struct FrameRef<'a> {
    pub kind: FrameKind,
    header: &'a [u8],
    pub runs: Vec<&'a [u8]>,
}

impl FrameRef<'_> {
    /// The header as its typed record.
    pub fn header<T: DeserializeOwned>(&self) -> Result<T, FrameError> {
        Ok(rmp_serde::from_slice(self.header)?)
    }
}

fn len32(n: usize) -> Result<u32, FrameError> {
    u32::try_from(n).map_err(|_| FrameError::TooLong(n))
}

/// Append one frame of `kind` to `out`: `header` as an array, then `runs` as they are.
pub fn encode(out: &mut Vec<u8>, kind: FrameKind, header: &impl Serialize, runs: &[&[u8]]) -> Result<(), FrameError> {
    let header = rmp_serde::to_vec(header)?;
    let count = u16::try_from(runs.len()).map_err(|_| FrameError::TooManyRuns(runs.len()))?;
    let body = 1 + 4 + header.len() + 2 + runs.iter().map(|r| 4 + r.len()).sum::<usize>();
    out.reserve(4 + body);
    out.extend_from_slice(&len32(body)?.to_le_bytes());
    out.push(kind as u8);
    out.extend_from_slice(&len32(header.len())?.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&count.to_le_bytes());
    for run in runs {
        out.extend_from_slice(&len32(run.len())?.to_le_bytes());
        out.extend_from_slice(run);
    }
    Ok(())
}

/// A cursor over a byte slice that refuses to read past its end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn span(&mut self, n: usize) -> Result<&'a [u8], FrameError> {
        let available = self.bytes.len() - self.at;
        if n > available {
            return Err(FrameError::Short { needed: n, available });
        }
        let part = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(part)
    }

    /// The next fixed-width field: exactly `N` bytes, or a short tail.
    fn field<const N: usize>(&mut self) -> Result<[u8; N], FrameError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.span(N)?);
        Ok(out)
    }

    fn u32(&mut self) -> Result<usize, FrameError> {
        Ok(u32::from_le_bytes(self.field()?) as usize)
    }

    fn u16(&mut self) -> Result<usize, FrameError> {
        Ok(u16::from_le_bytes(self.field()?) as usize)
    }
}

/// The frame at the start of `bytes`, and how many bytes it took. A tail shorter than its stated
/// length is `Short` (a write cut off mid-frame); parts that disagree with it are `Inconsistent`.
pub fn decode(bytes: &[u8]) -> Result<(FrameRef<'_>, usize), FrameError> {
    let mut outer = Cursor { bytes, at: 0 };
    let stated = outer.u32()?;
    let body = outer.span(stated)?;
    let mut inner = Cursor { bytes: body, at: 0 };
    let [kind] = inner.field()?;
    let kind = FrameKind::of(kind)?;
    let header_len = inner.u32()?;
    let header = inner.span(header_len)?;
    let count = inner.u16()?;
    let mut runs = Vec::with_capacity(count);
    for _ in 0..count {
        let run_len = inner.u32()?;
        runs.push(inner.span(run_len)?);
    }
    if inner.at != body.len() {
        return Err(FrameError::Inconsistent { parts: inner.at, stated });
    }
    Ok((FrameRef { kind, header, runs }, 4 + stated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Header {
        request_id: String,
        elapsed_ms: u64,
        context_window: Option<u32>,
    }

    // what this catches: a frame that does not round-trip (a header field lost or reordered, a run
    // altered or merged with its neighbour), a header that carries its field names (every row
    // repeating its schema), or a reader that panics or reads garbage on a truncated or corrupt tail
    // instead of refusing it by name.
    #[test]
    fn a_frame_round_trips_carries_no_field_names_and_refuses_a_bad_tail() {
        let header = Header { request_id: "r-1".into(), elapsed_ms: 42, context_window: None };
        let prompt = "the whole prompt, \"quoted\" and\nmultiline, as bytes".as_bytes();
        let wire = [0u8, 255, 10, 13];
        let mut out = Vec::new();
        encode(&mut out, FrameKind::Terminal, &header, &[prompt, &[], &wire]).expect("test: encodes");
        encode(&mut out, FrameKind::Submitted, &header, &[]).expect("test: a second frame");

        let (first, used) = decode(&out).expect("test: the first frame");
        assert_eq!(first.kind, FrameKind::Terminal);
        assert_eq!(first.header::<Header>().expect("test: header"), header);
        assert_eq!(first.runs, vec![prompt, &[][..], &wire[..]], "each run exactly as written, empty included");
        let (second, rest) = decode(&out[used..]).expect("test: the second frame");
        assert_eq!((second.kind, second.runs.len(), used + rest), (FrameKind::Submitted, 0, out.len()));

        let names = String::from_utf8_lossy(&out);
        assert!(!names.contains("request_id") && !names.contains("elapsed_ms"), "the header is positions, never field names");

        assert!(matches!(decode(&out[..used - 1]), Err(FrameError::Short { .. })), "a cut-off frame is a truncated tail");
        let mut unknown = out[..used].to_vec();
        unknown[4] = 9;
        assert!(matches!(decode(&unknown), Err(FrameError::UnknownKind(9))));
        let mut padded = out[..used].to_vec();
        let longer = (u32::from_le_bytes([padded[0], padded[1], padded[2], padded[3]]) + 1).to_le_bytes();
        padded[..4].copy_from_slice(&longer);
        padded.push(0);
        assert!(matches!(decode(&padded), Err(FrameError::Inconsistent { .. })), "parts that disagree with the length are refused");
    }
}
