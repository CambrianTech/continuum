//! The Rust edge's validation of a frame, the same rules as the engine's `ctm_validate` (server-continuum.cpp):
//! magic, schema version, schema identity, a known kind, the kind's exact size, zero reserved bytes, every tag in
//! range and every buffer's `offset + length` without overflow. Fields are read at the bound structs' own offsets
//! (`offset_of!`), little-endian, from bytes that need not be aligned; nothing is cast into an enum.

use llama::contract as c;
use std::mem::{offset_of, size_of};

/// Why a frame was refused; each the engine's `ctm_validation` value of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    Short,
    Magic,
    Version,
    Schema,
    Kind,
    Size,
    Reserved,
    Tag,
    BufferBounds,
}

impl FrameError {
    /// The engine's code for the same refusal (the cross-language test compares them).
    pub fn code(self) -> i32 {
        (match self {
            Self::Short => c::CTM_INVALID_SHORT,
            Self::Magic => c::CTM_INVALID_MAGIC,
            Self::Version => c::CTM_INVALID_VERSION,
            Self::Schema => c::CTM_INVALID_SCHEMA,
            Self::Kind => c::CTM_INVALID_KIND,
            Self::Size => c::CTM_INVALID_SIZE,
            Self::Reserved => c::CTM_INVALID_RESERVED,
            Self::Tag => c::CTM_INVALID_TAG,
            Self::BufferBounds => c::CTM_INVALID_BUFFER_BOUNDS,
        }) as i32
    }
}

/// A frame's kind, converted from its raw tag; an unknown tag is `FrameError::Kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Attach,
    EngineDescriptor,
    ActSubmit,
    ActDone,
    Cancel,
    Prefetch,
    Placed,
    PinRelease,
    Released,
    GenomeSet,
    LearnDuty,
    Refused,
}

impl FrameKind {
    fn from_tag(tag: u16) -> Option<Self> {
        Some(match u32::from(tag) {
            c::CTM_KIND_ATTACH => Self::Attach,
            c::CTM_KIND_ENGINE_DESCRIPTOR => Self::EngineDescriptor,
            c::CTM_KIND_ACT_SUBMIT => Self::ActSubmit,
            c::CTM_KIND_ACT_DONE => Self::ActDone,
            c::CTM_KIND_CANCEL => Self::Cancel,
            c::CTM_KIND_PREFETCH => Self::Prefetch,
            c::CTM_KIND_PLACED => Self::Placed,
            c::CTM_KIND_PIN_RELEASE => Self::PinRelease,
            c::CTM_KIND_RELEASED => Self::Released,
            c::CTM_KIND_GENOME_SET => Self::GenomeSet,
            c::CTM_KIND_LEARN_DUTY => Self::LearnDuty,
            c::CTM_KIND_REFUSED => Self::Refused,
            _ => return None,
        })
    }

    /// The kind's fixed size, the same as the engine's `ctm_frame_size`.
    pub fn size(self) -> usize {
        match self {
            Self::Attach => size_of::<c::ctm_attach>(),
            Self::EngineDescriptor => size_of::<c::ctm_engine_descriptor>(),
            Self::ActSubmit => size_of::<c::ctm_act_submit>(),
            Self::ActDone => size_of::<c::ctm_act_done>(),
            Self::Cancel => size_of::<c::ctm_cancel>(),
            Self::Prefetch => size_of::<c::ctm_prefetch>(),
            Self::Placed => size_of::<c::ctm_placed>(),
            Self::PinRelease => size_of::<c::ctm_pin_release>(),
            Self::Released => size_of::<c::ctm_released>(),
            Self::GenomeSet => size_of::<c::ctm_genome_set>(),
            Self::LearnDuty => size_of::<c::ctm_learn_duty>(),
            Self::Refused => size_of::<c::ctm_refused>(),
        }
    }
}

struct Frame<'a>(&'a [u8]);

impl Frame<'_> {
    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.0[at], self.0[at + 1]])
    }
    fn u32(&self, at: usize) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.0[at..at + 4]);
        u32::from_le_bytes(b)
    }
    fn u64(&self, at: usize) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.0[at..at + 8]);
        u64::from_le_bytes(b)
    }
    /// `offset + length` of the buffer_ref at `at` does not overflow.
    fn buffer_ok(&self, at: usize) -> bool {
        let offset = self.u64(at + offset_of!(c::ctm_buffer_ref, offset));
        let length = self.u64(at + offset_of!(c::ctm_buffer_ref, length));
        offset.checked_add(length).is_some()
    }
}

fn in_range(v: u32, valid: std::ops::Range<u32>) -> bool {
    valid.contains(&v)
}

/// Validate `bytes` as one frame of this build's schema; its kind when valid.
pub fn validate(bytes: &[u8]) -> Result<FrameKind, FrameError> {
    if bytes.len() < size_of::<c::ctm_frame_header>() {
        return Err(FrameError::Short);
    }
    let f = Frame(bytes);
    if f.u32(offset_of!(c::ctm_frame_header, magic)) != c::CTM_MAGIC {
        return Err(FrameError::Magic);
    }
    if u32::from(f.u16(offset_of!(c::ctm_frame_header, schema_version))) != c::CTM_SCHEMA_VERSION {
        return Err(FrameError::Version);
    }
    let schema_at = offset_of!(c::ctm_frame_header, schema);
    if bytes[schema_at..schema_at + 16] != c::SCHEMA_ID {
        return Err(FrameError::Schema);
    }
    let kind = FrameKind::from_tag(f.u16(offset_of!(c::ctm_frame_header, kind))).ok_or(FrameError::Kind)?;
    if usize::try_from(f.u32(offset_of!(c::ctm_frame_header, size))).ok() != Some(kind.size()) {
        return Err(FrameError::Size);
    }
    if bytes.len() < kind.size() {
        return Err(FrameError::Short);
    }
    if f.u32(offset_of!(c::ctm_frame_header, reserved)) != 0 {
        return Err(FrameError::Reserved);
    }
    check_tags(kind, &f).map(|()| kind)
}

fn check_tags(kind: FrameKind, f: &Frame<'_>) -> Result<(), FrameError> {
    let tag = |ok: bool| if ok { Ok(()) } else { Err(FrameError::Tag) };
    let zero = |ok: bool| if ok { Ok(()) } else { Err(FrameError::Reserved) };
    let bounds = |ok: bool| if ok { Ok(()) } else { Err(FrameError::BufferBounds) };
    match kind {
        FrameKind::EngineDescriptor => {
            zero(f.u16(offset_of!(c::ctm_engine_descriptor, reserved0)) == 0)?;
            bounds(f.buffer_ok(offset_of!(c::ctm_engine_descriptor, resources)))
        }
        FrameKind::ActSubmit => {
            zero(f.u16(offset_of!(c::ctm_act_submit, reserved0)) == 0)?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_act_submit, mode))), 1..c::CTM_ACT_MODE_COUNT))?;
            tag(f.u32(offset_of!(c::ctm_act_submit, flags)) & !c::CTM_ACT_FLAGS_ALL == 0)?;
            for at in [
                offset_of!(c::ctm_act_submit, prompt),
                offset_of!(c::ctm_act_submit, labels),
                offset_of!(c::ctm_act_submit, view),
                offset_of!(c::ctm_act_submit, sampling),
            ] {
                bounds(f.buffer_ok(at))?;
            }
            Ok(())
        }
        FrameKind::ActDone => {
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_act_done, outcome))), 1..c::CTM_OUTCOME_COUNT))?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_act_done, reason))), 0..c::CTM_REASON_COUNT))?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_act_done, mind_from))), 0..c::CTM_MIND_FROM_COUNT))?;
            tag(u32::from(f.u16(offset_of!(c::ctm_act_done, flags))) & !c::CTM_DONE_FLAGS_ALL == 0)?;
            bounds(f.buffer_ok(offset_of!(c::ctm_act_done, observations)))
        }
        FrameKind::Placed => {
            zero(f.u16(offset_of!(c::ctm_placed, reserved0)) == 0 && f.u32(offset_of!(c::ctm_placed, reserved1)) == 0)?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_placed, place))), 1..c::CTM_PLACE_COUNT))
        }
        FrameKind::Released => {
            zero(f.u16(offset_of!(c::ctm_released, reserved0)) == 0 && f.u32(offset_of!(c::ctm_released, reserved1)) == 0)?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_released, outcome))), 1..c::CTM_PIN_COUNT))
        }
        FrameKind::GenomeSet => bounds(f.buffer_ok(offset_of!(c::ctm_genome_set, layers))),
        FrameKind::LearnDuty => {
            zero(f.u16(offset_of!(c::ctm_learn_duty, reserved0)) == 0)?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_learn_duty, mode))), 1..c::CTM_LEARN_MODE_COUNT))?;
            tag(f.u32(offset_of!(c::ctm_learn_duty, share_ppm)) <= 1_000_000)
        }
        FrameKind::Refused => {
            zero(f.u32(offset_of!(c::ctm_refused, reserved0)) == 0)?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_refused, refused_kind))), 0..c::CTM_KIND_KIND_COUNT))?;
            tag(in_range(u32::from(f.u16(offset_of!(c::ctm_refused, reason))), 0..c::CTM_REASON_COUNT))
        }
        FrameKind::Attach | FrameKind::Cancel | FrameKind::Prefetch | FrameKind::PinRelease => Ok(()),
    }
}
