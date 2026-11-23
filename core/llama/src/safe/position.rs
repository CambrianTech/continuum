//! [`Pos`]: a token's position in a sequence, owned here so no caller handles a sign it cannot have.
//!
//! llama.cpp carries positions as a signed `llama_pos` (`int32_t`) and uses `-1` to mean "this
//! sequence holds nothing". A position is a count of the tokens before it: never negative, and never
//! past `i32::MAX` because llama.cpp cannot address one. So it is unsigned here, bounded on
//! construction, converted to the native type only where an FFI call takes one, and the `-1`
//! sentinel comes back as `None`.

use crate::sys;

/// A token's position in a sequence: the count of tokens before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos(u32);

/// A length no llama.cpp sequence can address (past `i32::MAX`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("position {0} is past what a llama.cpp sequence can address")]
pub struct PosOutOfRange(pub u64);

impl Pos {
    pub const ZERO: Self = Self(0);

    /// A position known at compile time: every `u16` is addressable, so this cannot fail.
    pub const fn at(count: u16) -> Self {
        Self(count as u32)
    }

    /// The count as an unsigned number.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The position `by` tokens later; refused past what a sequence can address.
    pub fn advanced(self, by: usize) -> Result<Self, PosOutOfRange> {
        Self::try_from(self.0 as usize + by)
    }

    /// As the FFI takes it. Infallible: a `Pos` never exceeds `i32::MAX`.
    pub(crate) const fn native(self) -> sys::llama_pos {
        self.0 as sys::llama_pos
    }

    /// From the FFI: `None` for llama.cpp's "holds nothing" (any negative value).
    pub(crate) fn from_native(native: sys::llama_pos) -> Option<Self> {
        u32::try_from(native).ok().map(Self)
    }
}

impl std::fmt::Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<usize> for Pos {
    type Error = PosOutOfRange;

    fn try_from(count: usize) -> Result<Self, Self::Error> {
        match i32::try_from(count) {
            Ok(native) => Ok(Self(native.unsigned_abs())),
            Err(_) => Err(PosOutOfRange(count as u64)),
        }
    }
}

impl From<Pos> for usize {
    fn from(pos: Pos) -> Self {
        pos.0 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a signed position handled as a number at call sites (2026-11-21:
    // `unsigned_abs` at a use site, and `-1` compared with `>= 0` to mean "empty"). The sentinel is
    // a typed absence, and a length past what llama.cpp addresses is refused, never wrapped.
    #[test]
    fn a_position_is_unsigned_bounded_and_empty_is_an_absence() {
        assert_eq!(Pos::from_native(-1), None, "llama.cpp's empty sequence");
        assert_eq!(Pos::from_native(41).map(Pos::get), Some(41));
        assert_eq!(Pos::try_from(122_368).map(Pos::native), Ok(122_368));
        assert_eq!(Pos::try_from(i32::MAX as usize + 1), Err(PosOutOfRange(i32::MAX as u64 + 1)));
        assert_eq!(Pos::try_from(10).and_then(|p| p.advanced(5)).map(usize::from), Ok(15));
    }
}
