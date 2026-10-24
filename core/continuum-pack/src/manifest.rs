//! The manifest: a package's one small document. Fields inline, every other part a
//! descriptor (`mediaType`, `digest`, `size`, the OCI descriptor shape), requirements first.
//! Stored as canonical JSON, so its id is a pure function of the value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical;
use crate::error::PackError;
use crate::id::ContentId;
use crate::requires::Requirement;

/// The media type of a manifest stored as a blob (a child part's descriptor carries it).
pub const MANIFEST_MEDIA: &str = "application/vnd.continuum.pack.manifest.v1+json";

/// The envelope format this build writes and reads. A newer one is refused, never guessed at.
pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub kind: String,
    pub schema: u32,
    pub requires: BTreeMap<String, Requirement>,
    pub fields: BTreeMap<String, Value>,
    pub parts: BTreeMap<String, Part>,
}

/// A non-field part: a blob, one child package, or an ordered list of children.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Part {
    Blob(Descriptor),
    Child(Descriptor),
    Children(Vec<Descriptor>),
}

/// Content by address: what it is, its id, how long it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: ContentId,
    pub size: u64,
}

impl Manifest {
    pub(crate) fn new(kind: &str, schema: u32) -> Self {
        Self {
            format: FORMAT,
            kind: kind.to_owned(),
            schema,
            requires: BTreeMap::new(),
            fields: BTreeMap::new(),
            parts: BTreeMap::new(),
        }
    }

    /// The canonical bytes (sorted keys, no whitespace).
    pub fn encode(&self) -> Result<Vec<u8>, PackError> {
        let value = serde_json::to_value(self).map_err(PackError::Manifest)?;
        Ok(canonical::to_vec(&value))
    }

    /// Parse a manifest, refusing an envelope newer than this build.
    pub fn decode(bytes: &[u8]) -> Result<Self, PackError> {
        let m: Self = serde_json::from_slice(bytes).map_err(PackError::Manifest)?;
        if m.format > FORMAT {
            return Err(PackError::UnknownFormat {
                found: m.format,
                known: FORMAT,
            });
        }
        Ok(m)
    }

    /// Every descriptor this manifest links to (`true` = a child manifest), in part-name order.
    pub(crate) fn links(&self) -> Vec<(bool, &Descriptor)> {
        let mut out = Vec::with_capacity(self.parts.len());
        for part in self.parts.values() {
            match part {
                Part::Blob(d) => out.push((false, d)),
                Part::Child(d) => out.push((true, d)),
                Part::Children(ds) => out.extend(ds.iter().map(|d| (true, d))),
            }
        }
        out
    }
}
