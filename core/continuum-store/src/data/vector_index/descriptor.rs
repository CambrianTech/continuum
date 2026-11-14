//! The native embedding descriptor: the producer's facts about a vector, written with every
//! row and built for every query, and the one rule that says which rows a query may score.
use super::{searchable_dimension, EmbedderId, Producer, VectorIndexError, NATIVE_ORDER};
use crate::orm::binary::{BinaryDescriptor, ElementType};
use std::fmt;
use std::num::NonZeroUsize;

/// The embedding space a vector lives in. Two vectors are comparable only inside one space:
/// a served embedder's output, or vectors a caller produced and sent on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmbeddingSpace {
    Embedder(EmbedderId),
    /// `vector/index` rows and `vector/search` queries the caller embedded itself.
    Caller,
}

impl Producer {
    /// The space this producer's vectors live in.
    pub(crate) fn space(&self) -> EmbeddingSpace {
        match self {
            Self::Embedder(id) => EmbeddingSpace::Embedder(id.clone()),
            Self::WireQuery | Self::IndexCommand => EmbeddingSpace::Caller,
        }
    }

    /// The inverse of the identity the descriptor stores.
    pub(super) fn from_identity(identity: &str) -> Self {
        match identity {
            Self::WIRE_QUERY => Self::WireQuery,
            Self::INDEX_COMMAND => Self::IndexCommand,
            embedder => Self::Embedder(EmbedderId::new(embedder)),
        }
    }
}

/// What a producer certifies about a stored or query vector. Every field is a fact the
/// producer wrote: unit length is certified by construction (no uncertified descriptor
/// exists), the elements are native f32, and the scale is the I8 quantization scale (None for
/// f32). A row's descriptor is stored beside its bytes; a query's is built by its producer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddingDescriptor {
    pub(crate) producer: Producer,
    pub(crate) element: ElementType,
    pub(crate) dimension: NonZeroUsize,
    pub(crate) scale: Option<f32>,
}

impl EmbeddingDescriptor {
    /// Reads a stored descriptor; refuses anything search cannot score as-is (an
    /// uncertified, non-native, or non-f32 vector).
    pub(crate) fn from_binary(binary: &BinaryDescriptor) -> Result<Self, VectorIndexError> {
        let dimension = searchable_dimension(binary)?;
        Ok(Self {
            producer: Producer::from_identity(&binary.producer),
            element: binary.element_type,
            dimension,
            scale: binary.scale,
        })
    }
}

/// The storage descriptor a producer writes with a certified unit f32 vector of `dimension`:
/// the one constructor for row and query descriptors alike (read back by
/// [`EmbeddingDescriptor::from_binary`]).
pub(crate) fn unit_f32_descriptor(producer: &Producer, dimension: NonZeroUsize) -> BinaryDescriptor {
    BinaryDescriptor {
        element_type: ElementType::F32,
        byte_order: NATIVE_ORDER,
        shape: vec![dimension.get() as u64],
        producer: producer.identity(),
        unit: true,
        scale: None,
    }
}

impl fmt::Display for EmbeddingDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}[{}] unit", self.producer.identity(), self.element, self.dimension)
    }
}

/// The one rule: a query may score rows only in its own embedding space, with the same
/// element type, dimension and scale. Anything else is a comparison across spaces.
pub(crate) fn compatible(query: &EmbeddingDescriptor, rows: &EmbeddingDescriptor) -> bool {
    query.producer.space() == rows.producer.space()
        && query.element == rows.element
        && query.dimension == rows.dimension
        && query.scale == rows.scale
}
