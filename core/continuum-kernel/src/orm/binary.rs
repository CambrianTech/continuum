//! Producer-owned native payloads. Only the small descriptor is decoded by storage.
//! Payload bytes are never interpreted, normalized, or converted here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ElementType { U8 = 1, I8 = 2, F16 = 3, Bf16 = 4, F32 = 5, F64 = 6 }

impl ElementType {
    pub fn width(self) -> usize {
        match self { Self::U8 | Self::I8 => 1, Self::F16 | Self::Bf16 => 2, Self::F32 => 4, Self::F64 => 8 }
    }
    fn read(tag: u8) -> Result<Self, String> {
        match tag { 1 => Ok(Self::U8), 2 => Ok(Self::I8), 3 => Ok(Self::F16), 4 => Ok(Self::Bf16), 5 => Ok(Self::F32), 6 => Ok(Self::F64), _ => Err("unknown binary element type".into()) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ByteOrder { Little = 1, Big = 2 }

/// Describes a contiguous producer buffer, independently of its numeric precision.
/// Shape validation visits dimensions, never payload elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryDescriptor {
    pub element_type: ElementType,
    pub byte_order: ByteOrder,
    pub shape: Vec<u64>,
    pub producer: String,
}

impl BinaryDescriptor {
    pub fn byte_len(&self) -> Result<usize, String> {
        if self.shape.is_empty() || self.producer.is_empty() { return Err("binary descriptor requires shape and producer identity".into()); }
        self.shape.iter().try_fold(self.element_type.width(), |size, &dimension| {
            size.checked_mul(usize::try_from(dimension).map_err(|_| "binary dimension exceeds address space")?)
                .ok_or_else(|| "binary shape exceeds address space".into())
        })
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, String> {
        self.byte_len()?;
        let rank = u8::try_from(self.shape.len()).map_err(|_| "binary rank exceeds descriptor format")?;
        let producer_len = u32::try_from(self.producer.len()).map_err(|_| "producer identity exceeds descriptor format")?;
        let mut header = vec![1, self.element_type as u8, self.byte_order as u8, rank];
        header.extend_from_slice(&producer_len.to_le_bytes());
        for dimension in &self.shape { header.extend_from_slice(&dimension.to_le_bytes()); }
        header.extend_from_slice(self.producer.as_bytes());
        Ok(header)
    }

    pub(super) fn decode(header: &[u8]) -> Result<Self, String> {
        let prefix = header.get(..8).ok_or("truncated binary descriptor")?;
        if prefix[0] != 1 { return Err("unsupported binary descriptor version".into()); }
        let element_type = ElementType::read(prefix[1])?;
        let byte_order = match prefix[2] { 1 => ByteOrder::Little, 2 => ByteOrder::Big, _ => return Err("unknown binary byte order".into()) };
        let rank = usize::from(prefix[3]);
        let producer_len = u32::from_le_bytes(prefix[4..8].try_into().map_err(|_| "invalid producer length")?) as usize;
        let dimensions_end = 8 + rank * 8;
        if dimensions_end.checked_add(producer_len) != Some(header.len()) { return Err("binary descriptor length mismatch".into()); }
        let dimensions = header.get(8..dimensions_end).ok_or("truncated binary dimensions")?;
        let shape = dimensions.chunks_exact(8).map(|dimension| {
            Ok(u64::from_le_bytes(dimension.try_into().map_err(|_| "invalid binary dimension")?))
        }).collect::<Result<Vec<_>, String>>()?;
        let producer = std::str::from_utf8(&header[dimensions_end..]).map_err(|_| "producer identity is not UTF-8")?.to_owned();
        let descriptor = Self { element_type, byte_order, shape, producer };
        descriptor.byte_len()?;
        Ok(descriptor)
    }
}

/// Moves the producer allocation into custody without inspecting its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug)]
pub struct BinaryRecord {
    pub id: String,
    pub value: Option<BinaryValue>,
}
