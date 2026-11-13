//! The producer's one normalization: certify `unit = true` at write, never per query.
use super::NATIVE_ORDER;
use crate::orm::binary::{BinaryDescriptor, BinaryValue, ElementType};
use std::fmt;

/// Why a producer's vector cannot be certified unit-length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnitVectorError {
    /// No components: there is no direction to normalize.
    Empty,
    /// A component is NaN or infinite, or the squared length overflows f32.
    NonFinite,
    /// The all-zero vector has no direction; normalizing it would produce NaN.
    Zero,
    /// The storage seam refused the descriptor (shape exceeds its format).
    Descriptor(String),
}

impl fmt::Display for UnitVectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "an empty vector has no direction to normalize"),
            Self::NonFinite => write!(f, "a vector component is not finite, or its squared length overflows f32"),
            Self::Zero => write!(f, "the zero vector has no direction; refusing to store NaN"),
            Self::Descriptor(error) => write!(f, "unit vector descriptor: {error}"),
        }
    }
}

impl std::error::Error for UnitVectorError {}

/// Normalizes the producer's f32 output once and moves it into a native `unit = true`
/// payload. The scale is fused into the single copy that moves the producer's values into
/// the payload's bytes: one read pass for the length, one write pass for the bytes, no
/// f64, no intermediate buffer. `values` is read twice, so it must be cheap to clone
/// (a slice iterator, or the wire's f64 -> f32 map over a slice).
pub(crate) fn unit_f32<I>(values: I, producer: &str) -> Result<BinaryValue, UnitVectorError>
where
    I: ExactSizeIterator<Item = f32> + Clone,
{
    let dimension = values.len();
    if dimension == 0 {
        return Err(UnitVectorError::Empty);
    }
    let squared: f32 = values.clone().map(|value| value * value).sum();
    if !squared.is_finite() {
        return Err(UnitVectorError::NonFinite);
    }
    if squared == 0.0 {
        return Err(UnitVectorError::Zero);
    }
    // The one square root, paid by the producer at write so no query ever pays it.
    let inverse = squared.sqrt().recip();
    let byte_len = dimension
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| UnitVectorError::Descriptor("vector byte length exceeds address space".into()))?;
    let mut bytes = Vec::with_capacity(byte_len);
    for value in values {
        bytes.extend_from_slice(&(value * inverse).to_ne_bytes());
    }
    let shape = u64::try_from(dimension)
        .map_err(|_| UnitVectorError::Descriptor("vector dimension exceeds u64".into()))?;
    BinaryValue::new(
        BinaryDescriptor {
            element_type: ElementType::F32,
            byte_order: NATIVE_ORDER,
            shape: vec![shape],
            producer: producer.to_owned(),
            unit: true,
            scale: None,
        },
        bytes,
    )
    .map_err(UnitVectorError::Descriptor)
}
