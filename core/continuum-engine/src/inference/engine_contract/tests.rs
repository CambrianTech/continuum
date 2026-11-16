//! what this catches: the two edges of the core<->engine contract drifting. Every case runs the Rust validator and
//! the engine's own C validator (`ctm_validate`, linked from the fork) over the SAME bytes and requires the same
//! verdict, and the schema identity both sides compiled in to be equal (CONTINUUM-ENGINE 4.11.3).

use super::*;
use llama::contract as c;

fn bytes_of<T>(frame: &T) -> &[u8] {
    // test: a bound POD frame viewed as its bytes, exactly what crosses the wire
    unsafe { std::slice::from_raw_parts((frame as *const T).cast::<u8>(), std::mem::size_of::<T>()) }
}

fn engine_verdict(bytes: &[u8]) -> i32 {
    // test: the fork's validator over the same bytes
    unsafe { c::ctm_validate(bytes.as_ptr().cast(), bytes.len()) }
}

/// Rust and C agree on `bytes`; returns the shared verdict as the engine's code.
fn agree(bytes: &[u8]) -> i32 {
    let rust = validate(bytes).map_or_else(FrameError::code, |_| c::CTM_VALID as i32);
    let engine = engine_verdict(bytes);
    assert_eq!(rust, engine, "the Rust and engine edges disagree on the same bytes");
    rust
}

fn header(kind: u32, size: usize) -> c::ctm_frame_header {
    c::ctm_frame_header {
        magic: c::CTM_MAGIC,
        schema_version: c::CTM_SCHEMA_VERSION as u16,
        kind: kind as u16,
        size: size as u32,
        reserved: 0,
        schema: c::ctm_id { bytes: c::SCHEMA_ID },
    }
}

#[test]
fn both_edges_compiled_in_the_same_schema() {
    // test: reading the linked static
    let engine = unsafe { c::ctm_schema_id.bytes };
    assert_eq!(engine, c::SCHEMA_ID, "core and engine were built from different contract headers");
}

#[test]
fn both_edges_give_every_frame_the_same_verdict() {
    let mut p = c::ctm_prefetch { header: header(c::CTM_KIND_PREFETCH, std::mem::size_of::<c::ctm_prefetch>()), ..Default::default() };
    assert_eq!(agree(bytes_of(&p)), c::CTM_VALID as i32);
    let one_short = std::mem::size_of::<c::ctm_frame_header>() - 1;
    assert_eq!(agree(&bytes_of(&p)[..one_short]), c::CTM_INVALID_SHORT as i32);
    p.header.magic ^= 1;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_MAGIC as i32);
    p.header.magic ^= 1;
    p.header.schema_version = 2;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_VERSION as i32);
    p.header.schema_version = c::CTM_SCHEMA_VERSION as u16;
    p.header.schema.bytes[0] ^= 1;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_SCHEMA as i32);
    p.header.schema.bytes[0] ^= 1;
    p.header.kind = 999;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_KIND as i32);
    p.header.kind = c::CTM_KIND_PREFETCH as u16;
    p.header.size += 8;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_SIZE as i32);
    p.header.size -= 8;
    p.header.reserved = 1;
    assert_eq!(agree(bytes_of(&p)), c::CTM_INVALID_RESERVED as i32);

    let mut a = c::ctm_act_submit { header: header(c::CTM_KIND_ACT_SUBMIT, std::mem::size_of::<c::ctm_act_submit>()), ..Default::default() };
    a.mode = c::CTM_ACT_GENERATE as u16;
    a.flags = c::CTM_ACT_LEARN_PIN;
    a.act = ActId::mint().to_wire();
    assert_eq!(agree(bytes_of(&a)), c::CTM_VALID as i32);
    a.mode = 7;
    assert_eq!(agree(bytes_of(&a)), c::CTM_INVALID_TAG as i32, "an out-of-range enum is refused, never cast");
    a.mode = c::CTM_ACT_LEARN as u16;
    a.flags = 1 << 9;
    assert_eq!(agree(bytes_of(&a)), c::CTM_INVALID_TAG as i32, "an unknown flag bit");
    a.flags = 0;
    a.prompt.offset = u64::MAX;
    a.prompt.length = 2;
    assert_eq!(agree(bytes_of(&a)), c::CTM_INVALID_BUFFER_BOUNDS as i32, "offset + length overflows");

    let mut l = c::ctm_learn_duty { header: header(c::CTM_KIND_LEARN_DUTY, std::mem::size_of::<c::ctm_learn_duty>()), ..Default::default() };
    l.mode = c::CTM_LEARN_DREAM as u16;
    l.share_ppm = 1_000_001;
    assert_eq!(agree(bytes_of(&l)), c::CTM_INVALID_TAG as i32, "a share past the whole");
}

#[test]
fn an_id_crosses_as_its_own_sixteen_bytes() {
    let act = ActId::mint();
    assert_eq!(act.to_wire().bytes, *act.uuid().as_bytes());
    assert_eq!(ActId::from_wire(&act.to_wire()), act);
    assert_eq!(act.uuid().get_version_num(), 7);
}
