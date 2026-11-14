//! Producer-owned native payloads. Only the small descriptor is decoded by storage.
//! Payload bytes are never interpreted, normalized, or converted here.

/// A bounded native read. Absence of a cursor means the beginning, not missing input.
#[derive(Debug, Clone, Copy)]
pub struct BinaryPageRequest<'a> {
    pub collection: &'a str,
    pub field: &'a str,
    pub after: Option<uuid::Uuid>,
    pub limit: std::num::NonZeroUsize,
}

/// Borrowed destination views; the caller retains allocation and buffer custody.
pub struct BinaryPageOutput<'a> {
    pub bytes: &'a mut Vec<u8>,
    pub ids: &'a mut Vec<uuid::Uuid>,
}
/// Evaluated atomically by the write owner, never by an earlier read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryWriteCondition {
    Replace,
    /// No change unless every supplied payload/descriptor pair is absent and the row exists.
    IfAbsent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ElementType {
    U8 = 1, I8 = 2, F16 = 3, Bf16 = 4, F32 = 5, F64 = 6,
    /// Packed low-bit-first; consumers ignore trailing bits beyond the declared shape.
    Bit = 7,
}

impl ElementType {
    fn payload_len(self, elements: usize) -> Result<usize, String> {
        let width = match self {
            Self::Bit => return Ok(elements / 8 + usize::from(elements % 8 != 0)),
            Self::U8 | Self::I8 => 1, Self::F16 | Self::Bf16 => 2, Self::F32 => 4, Self::F64 => 8,
        };
        elements.checked_mul(width).ok_or_else(|| "binary shape exceeds address space".into())
    }
    fn read(tag: u8) -> Result<Self, String> {
        match tag { 1 => Ok(Self::U8), 2 => Ok(Self::I8), 3 => Ok(Self::F16), 4 => Ok(Self::Bf16), 5 => Ok(Self::F32), 6 => Ok(Self::F64), 7 => Ok(Self::Bit), _ => Err("unknown binary element type".into()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ByteOrder { Little = 1, Big = 2 }

/// Describes a contiguous producer buffer, independently of its numeric precision.
/// Shape validation visits dimensions, never payload elements.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryDescriptor {
    pub element_type: ElementType,
    pub byte_order: ByteOrder,
    pub shape: Vec<u64>,
    pub producer: String,
    /// Producer assertion of L2 normalization, never inferred by storage.
    pub unit: bool,
    /// Symmetric I8 quantization scale. None declares no scale, not an implicit 1.
    pub scale: Option<f32>,
}

impl BinaryDescriptor {
    pub fn byte_len(&self) -> Result<usize, String> {
        if self.shape.is_empty() || self.producer.is_empty() { return Err("binary descriptor requires shape and producer identity".into()); }
        if let Some(scale) = self.scale {
            if self.element_type != ElementType::I8 || !scale.is_finite() || scale <= 0.0 {
                return Err("binary scale requires I8 and a finite positive value".into());
            }
        }
        if self.unit && self.element_type == ElementType::Bit {
            return Err("packed bits cannot declare numerical L2 normalization".into());
        }
        let elements = self.shape.iter().try_fold(1_usize, |size, &dimension| {
            size.checked_mul(usize::try_from(dimension).map_err(|_| "binary dimension exceeds address space")?)
                .ok_or("binary shape exceeds address space")
        })?;
        self.element_type.payload_len(elements)
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, String> {
        self.byte_len()?;
        let rank = u8::try_from(self.shape.len()).map_err(|_| "binary rank exceeds descriptor format")?;
        let producer_len = u32::try_from(self.producer.len()).map_err(|_| "producer identity exceeds descriptor format")?;
        let mut header = vec![2, self.element_type as u8, self.byte_order as u8, rank];
        header.extend_from_slice(&producer_len.to_le_bytes());
        header.push(u8::from(self.unit) | (u8::from(self.scale.is_some()) << 1));
        if let Some(scale) = self.scale { header.extend_from_slice(&scale.to_le_bytes()); }
        for dimension in &self.shape { header.extend_from_slice(&dimension.to_le_bytes()); }
        header.extend_from_slice(self.producer.as_bytes());
        Ok(header)
    }

    pub(super) fn decode(header: &[u8]) -> Result<Self, String> {
        let prefix = header.get(..9).ok_or("truncated binary descriptor")?;
        if prefix[0] != 2 { return Err("unsupported binary descriptor version".into()); }
        let element_type = ElementType::read(prefix[1])?;
        let byte_order = match prefix[2] { 1 => ByteOrder::Little, 2 => ByteOrder::Big, _ => return Err("unknown binary byte order".into()) };
        let rank = usize::from(prefix[3]);
        let producer_len = u32::from_le_bytes(prefix[4..8].try_into().map_err(|_| "invalid producer length")?) as usize;
        let flags = prefix[8];
        if flags & !3 != 0 { return Err("unknown binary descriptor flags".into()); }
        let unit = flags & 1 != 0;
        let (scale, dimensions_start) = if flags & 2 != 0 {
            let bytes = header.get(9..13).ok_or("truncated binary scale")?;
            (Some(f32::from_le_bytes(bytes.try_into().map_err(|_| "invalid binary scale")?)), 13)
        } else { (None, 9) };
        let dimensions_end = dimensions_start + rank * 8;
        if dimensions_end.checked_add(producer_len) != Some(header.len()) { return Err("binary descriptor length mismatch".into()); }
        let dimensions = header.get(dimensions_start..dimensions_end).ok_or("truncated binary dimensions")?;
        let shape = dimensions.chunks_exact(8).map(|dimension| {
            Ok(u64::from_le_bytes(dimension.try_into().map_err(|_| "invalid binary dimension")?))
        }).collect::<Result<Vec<_>, String>>()?;
        let producer = std::str::from_utf8(&header[dimensions_end..]).map_err(|_| "producer identity is not UTF-8")?.to_owned();
        let descriptor = Self { element_type, byte_order, shape, producer, unit, scale };
        descriptor.byte_len()?;
        Ok(descriptor)
    }
}

/// Moves the producer allocation into custody without inspecting its contents.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryValue {
    descriptor: BinaryDescriptor,
    bytes: Vec<u8>,
}

impl BinaryValue {
    pub fn new(descriptor: BinaryDescriptor, bytes: Vec<u8>) -> Result<Self, String> {
        if descriptor.byte_len()? != bytes.len() { return Err("binary payload length disagrees with descriptor".into()); }
        Ok(Self { descriptor, bytes })
    }
    pub fn descriptor(&self) -> &BinaryDescriptor { &self.descriptor }
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    pub fn into_parts(self) -> (BinaryDescriptor, Vec<u8>) { (self.descriptor, self.bytes) }
}

pub(super) fn descriptor_column(field: &str) -> String {
    format!("{}__descriptor", super::adapter::naming::to_snake_case(field))
}

/// A typed page cursor must have exactly the same ordering as persisted identity text.
/// Validate canonical spelling on native writes and reads; never silently rewrite it.
pub(super) fn native_identity(text: &str) -> Result<uuid::Uuid, String> {
    let id = uuid::Uuid::parse_str(text).map_err(|e| format!("invalid native row identity: {e}"))?;
    let mut canonical = [0_u8; 36];
    if id.hyphenated().encode_lower(&mut canonical) != text {
        return Err("native row identity requires canonical lowercase hyphenated UUID".into());
    }
    Ok(id)
}