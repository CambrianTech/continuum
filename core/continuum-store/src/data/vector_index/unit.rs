//! The producer's one normalization: certify `unit = true` at write, never per query.
use super::{Producer, NATIVE_ORDER};
use crate::orm::binary::{BinaryDescriptor, BinaryDescriptorError, BinaryValue, ElementType};

/// Why a producer's vector cannot be certified unit-length.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum UnitVectorError {
    /// No components: there is no direction to normalize.
    #[error("an empty vector has no direction to normalize")]
    Empty,
    /// A component is NaN or infinite, or the squared length overflows f32.
    #[error("a vector component is not finite, or its squared length overflows f32")]
    NonFinite,
    /// The all-zero vector has no direction; normalizing it would produce NaN.
    #[error("the zero vector has no direction; refusing to store NaN")]
    Zero,
    /// The vector's byte length or dimension does not fit the host or the descriptor format.
    #[error("a vector of {dimension} components exceeds the address space")]
    LengthOverflow { dimension: usize },
    /// The storage seam refused the descriptor.
    #[error(transparent)]
    Descriptor(#[from] BinaryDescriptorError),
}

/// Normalizes the producer's f32 output once and moves it into a native `unit = true`
/// payload. The scale is fused into the single copy that moves the producer's values into
/// the payload's bytes: one read pass for the length, one write pass for the bytes, no
/// f64, no intermediate buffer. `values` is read twice, so it must be cheap to clone
/// (a slice iterator, or the wire's f64 -> f32 map over a slice).
pub(crate) fn unit_f32<I>(values: I, producer: &Producer) -> Result<BinaryValue, UnitVectorError>
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
    let overflow = UnitVectorError::LengthOverflow { dimension };
    let byte_len = dimension.checked_mul(std::mem::size_of::<f32>()).ok_or_else(|| overflow.clone())?;
    let shape = u64::try_from(dimension).map_err(|_| overflow)?;
    let mut bytes = Vec::with_capacity(byte_len);
    for value in values {
        bytes.extend_from_slice(&(value * inverse).to_ne_bytes());
    }
    let descriptor = BinaryDescriptor {
        element_type: ElementType::F32,
        byte_order: NATIVE_ORDER,
        shape: vec![shape],
        producer: producer.identity(),
        unit: true,
        scale: None,
    };
    Ok(BinaryValue::new(descriptor, bytes)?)
}
