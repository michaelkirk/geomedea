use serde::de::DeserializeOwned;
use std::fmt::{Debug, Formatter};
use std::io::Read;
use std::ops::Range;

use crate::io::CountingReader;
use crate::packed_r_tree::{Node, PackedRTree};
use crate::writer::PageHeader;
use crate::{deserialize_from, Feature, Header, Result};

struct CountingDeserializer<'a> {
    counting_reader: CountingReader<&'a [u8]>,
    original_bytes: &'a [u8],
}

impl<'a> CountingDeserializer<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            counting_reader: CountingReader::new(bytes, "CountingDeserializer"),
            original_bytes: bytes,
        }
    }

    fn is_empty(&self) -> bool {
        self.counting_reader.total_bytes_read() as usize == self.original_bytes.len()
    }

    fn deserialize_index(
        &mut self,
        byte_len: usize,
        num_leaf_nodes: u64,
        label: impl Into<String>,
    ) -> Result<Counted<'a, IndexInspector>> {
        let start = self.counting_reader.total_bytes_read() as usize;
        let mut bytes = vec![0; byte_len];
        self.counting_reader.read_exact(&mut bytes)?;
        let location = start..self.counting_reader.total_bytes_read() as usize;
        let item = IndexInspector {
            tree: PackedRTree::new(num_leaf_nodes),
            bytes,
        };
        Ok(Counted {
            label: label.into(),
            item,
            bytes: ByteFormatter(&self.original_bytes[location.clone()]),
            location,
        })
    }

    fn deserialize<D: DeserializeOwned>(
        &mut self,
        label: impl Into<String>,
    ) -> Result<Counted<'a, D>> {
        let start = self.counting_reader.total_bytes_read() as usize;
        let item: D = deserialize_from(&mut self.counting_reader)?;
        let location = start..self.counting_reader.total_bytes_read() as usize;
        Ok(Counted {
            label: label.into(),
            item,
            bytes: ByteFormatter(&self.original_bytes[location.clone()]),
            location,
        })
    }
}

pub(crate) struct ByteFormatter<'a>(pub &'a [u8]);

impl Debug for ByteFormatter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02X?}", self.0)
    }
}

pub(crate) struct IndexInspector {
    tree: PackedRTree,
    bytes: Vec<u8>,
}

impl Debug for IndexInspector {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let node_size = Node::serialized_size();
        // REVIEW: level # is reversed from the teriminology we use in PackedRTree. What's typical? Is the root level 0 or is the feature row level 0?
        for (level, level_byte_range) in self.tree.byte_ranges_by_level().into_iter().enumerate() {
            writeln!(
                f,
                "level {level}: {:?}",
                ByteFormatter(&self.bytes[level_byte_range.clone()])
            )?;
            let count = self.bytes[level_byte_range.clone()].chunks(node_size).len();
            for (chunk_idx, node_chunk) in
                self.bytes[level_byte_range].chunks(node_size).enumerate()
            {
                let last_chunk = chunk_idx == count - 1;
                let node: Node = deserialize_from(node_chunk).unwrap();
                if last_chunk {
                    write!(f, "{node:?}")?;
                } else {
                    write!(f, "{node:?}, ")?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

struct Counted<'a, T> {
    label: String,
    location: Range<usize>,
    bytes: ByteFormatter<'a>,
    item: T,
}

impl<T: Debug> Debug for Counted<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "== {} ==", self.label)?;
        if self.location.len() == 1 {
            write!(
                f,
                "{:?}: {:#04X?}:\n{:#?}",
                self.location.start, self.bytes, self.item
            )
        } else {
            write!(
                f,
                "{:?} ({} bytes): {:#04X?}:\n{:#?}",
                self.location,
                self.location.len(),
                self.bytes,
                self.item
            )
        }
    }
}

struct InspectedPage<'a> {
    page_header: Counted<'a, PageHeader>,
    // (feature size, feature)
    features: Vec<(Counted<'a, u64>, Counted<'a, Feature>)>,
}
pub struct Inspector<'a> {
    header: Counted<'a, Header>,
    // TODO: format this more nicely
    index: Counted<'a, IndexInspector>,
    // page -> [(feature_len, feature),]
    pages: Vec<InspectedPage<'a>>,
}

impl<'a> Inspector<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let mut deserializer = CountingDeserializer::new(bytes);

        let header = deserializer.deserialize::<Header>("header")?;
        let tree = PackedRTree::new(header.item.feature_count);
        let index = deserializer.deserialize_index(
            tree.index_size() as usize,
            header.item.feature_count,
            "index",
        )?;

        let mut pages = vec![];
        for page_idx in 0..header.item.page_count {
            let page_header =
                deserializer.deserialize::<PageHeader>(format!("page #{page_idx}"))?;
            let mut features = vec![];
            for feature_idx in 0..page_header.item.feature_count() {
                let feature_size =
                    deserializer.deserialize::<u64>(format!("feature #{feature_idx} len"))?;
                let feature =
                    deserializer.deserialize::<Feature>(format!("feature #{feature_idx}"))?;
                features.push((feature_size, feature));
            }
            pages.push(InspectedPage {
                page_header,
                features,
            });
        }
        assert!(deserializer.is_empty());

        Ok(Self {
            header,
            index,
            pages,
        })
    }
}

impl Debug for Inspector<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self.header)?;
        writeln!(f, "{:?}", self.index)?;
        for InspectedPage {
            page_header,
            features,
        } in &self.pages
        {
            writeln!(f, "{page_header:?}")?;
            for (feature_len, feature) in features {
                writeln!(f, "{feature_len:?}")?;
                writeln!(f, "{feature:?}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{Properties, PropertyValue};
    use crate::{wkt, Writer};

    #[test]
    fn inspect() {
        let multipoint = wkt! { MULTIPOINT(1 2,11 12,-1 -2) };
        let mut bytes = vec![];
        {
            let mut writer = Writer::new(&mut bytes, false).unwrap();
            writer.set_page_size_goal(100);

            for (idx, point) in multipoint.points().iter().enumerate() {
                let geometry = point.clone().into();
                let mut properties = Properties::empty();
                properties.insert(
                    "some_prop".to_string(),
                    PropertyValue::String(format!("value-{idx}")),
                );

                let feature = Feature::new(geometry, properties);
                writer.add_feature(&feature).unwrap();
            }
            writer.finish().unwrap();
        }

        println!(
            "all bytes: {formatted_bytes:?}",
            formatted_bytes = ByteFormatter(&bytes)
        );

        let inspector = Inspector::new(&bytes).unwrap();
        let output = format!("{:?}", inspector);
        println!("{}", output);
        let expected = r#"== header ==
0..17 (17 bytes): 0x[00, 02, 00, 00, 00, 00, 00, 00, 00, 03, 00, 00, 00, 00, 00, 00, 00]:
Header {
    is_compressed: false,
    page_count: 2,
    feature_count: 3,
}
== index ==
17..129 (112 bytes): 0x[80, 69, 67, FF, 00, D3, CE, FE, 80, 77, 8E, 06, 00, 0E, 27, 07, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, 77, 8E, 06, 00, 0E, 27, 07, 80, 77, 8E, 06, 00, 0E, 27, 07, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, 96, 98, 00, 00, 2D, 31, 01, 80, 96, 98, 00, 00, 2D, 31, 01, 00, 00, 00, 00, 00, 00, 00, 00, 59, 00, 00, 00, 80, 69, 67, FF, 00, D3, CE, FE, 80, 69, 67, FF, 00, D3, CE, FE, BE, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00]:
level 0: 0x[80, 69, 67, FF, 00, D3, CE, FE, 80, 77, 8E, 06, 00, 0E, 27, 07, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00]
Node { bounds: RECT(-1 -2, 11 12), offset: FeatureLocation { page_starting_offset: 0, feature_offset: 0 } }
level 1: 0x[80, 77, 8E, 06, 00, 0E, 27, 07, 80, 77, 8E, 06, 00, 0E, 27, 07, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, 96, 98, 00, 00, 2D, 31, 01, 80, 96, 98, 00, 00, 2D, 31, 01, 00, 00, 00, 00, 00, 00, 00, 00, 59, 00, 00, 00, 80, 69, 67, FF, 00, D3, CE, FE, 80, 69, 67, FF, 00, D3, CE, FE, BE, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00]
Node { bounds: RECT(11 12, 11 12), offset: FeatureLocation { page_starting_offset: 0, feature_offset: 0 } }, Node { bounds: RECT(1 2, 1 2), offset: FeatureLocation { page_starting_offset: 0, feature_offset: 89 } }, Node { bounds: RECT(-1 -2, -1 -2), offset: FeatureLocation { page_starting_offset: 190, feature_offset: 0 } }

== page #0 ==
129..141 (12 bytes): 0x[B2, 00, 00, 00, B2, 00, 00, 00, 02, 00, 00, 00]:
PageHeader {
    encoded_page_length: 178,
    decoded_page_length: 178,
    feature_count: 2,
}
== feature #0 len ==
141..149 (8 bytes): 0x[51, 00, 00, 00, 00, 00, 00, 00]:
81
== feature #0 ==
149..230 (81 bytes): 0x[00, 00, 00, 00, 80, 77, 8E, 06, 00, 0E, 27, 07, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 0C, 00, 00, 00, 07, 00, 00, 00, 00, 00, 00, 00, 76, 61, 6C, 75, 65, 2D, 31]:
Feature {
    geometry: POINT(11 12),
    properties: Properties {
        some_prop: String(
            "value-1",
        ),
    },
}
== feature #1 len ==
230..238 (8 bytes): 0x[51, 00, 00, 00, 00, 00, 00, 00]:
81
== feature #1 ==
238..319 (81 bytes): 0x[00, 00, 00, 00, 80, 96, 98, 00, 00, 2D, 31, 01, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 0C, 00, 00, 00, 07, 00, 00, 00, 00, 00, 00, 00, 76, 61, 6C, 75, 65, 2D, 30]:
Feature {
    geometry: POINT(1 2),
    properties: Properties {
        some_prop: String(
            "value-0",
        ),
    },
}
== page #1 ==
319..331 (12 bytes): 0x[59, 00, 00, 00, 59, 00, 00, 00, 01, 00, 00, 00]:
PageHeader {
    encoded_page_length: 89,
    decoded_page_length: 89,
    feature_count: 1,
}
== feature #0 len ==
331..339 (8 bytes): 0x[51, 00, 00, 00, 00, 00, 00, 00]:
81
== feature #0 ==
339..420 (81 bytes): 0x[00, 00, 00, 00, 80, 69, 67, FF, 00, D3, CE, FE, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 01, 00, 00, 00, 00, 00, 00, 00, 09, 00, 00, 00, 00, 00, 00, 00, 73, 6F, 6D, 65, 5F, 70, 72, 6F, 70, 0C, 00, 00, 00, 07, 00, 00, 00, 00, 00, 00, 00, 76, 61, 6C, 75, 65, 2D, 32]:
Feature {
    geometry: POINT(-1 -2),
    properties: Properties {
        some_prop: String(
            "value-2",
        ),
    },
}
"#;
        assert_eq!(output, expected);
    }
}
