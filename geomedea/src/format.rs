use crate::serialized_size;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureLocation {
    /// How far into the feature data does this feature's page start?
    pub(crate) page_starting_offset: u64,
    /// The byte offset of this feature within its (uncompressed) page
    pub(crate) feature_offset: u32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct PageHeader {
    /// The number of bytes on disk. The actual content bytes might be more or less if
    /// the page is compressed. (hopefully less!)
    encoded_page_length: u32,

    /// The number of bytes after decompression. This is the number of bytes that will be
    /// read from the page to decode the features.k
    decoded_page_length: u32,
    feature_count: u32,
}

impl PageHeader {
    #[cfg(feature = "writer")]
    pub fn new(encoded_page_length: u32, decoded_page_length: u32, feature_count: u32) -> Self {
        Self {
            encoded_page_length,
            decoded_page_length,
            feature_count,
        }
    }

    pub fn serialized_size() -> usize {
        // Assumes the PageHeader serialization is fixed. We'll have to revisit if this every changes.
        let value = serialized_size(&Self::default()).expect("valid serialization size");
        debug_assert_eq!(value, 12, "If PageHeader fields are changed, this assertion can be updated, but it *must* remain a fixed size - e.g. no dynamically sized types like a Vec");
        value as usize
    }
    pub fn encoded_page_length(&self) -> u32 {
        self.encoded_page_length
    }
    pub fn feature_count(&self) -> u32 {
        self.feature_count
    }
    pub fn decoded_page_length(&self) -> u32 {
        self.decoded_page_length
    }
}
