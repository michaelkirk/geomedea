use crate::packed_r_tree::{Node, PackedRTree, BRANCHING_FACTOR};
use crate::{deserialize_from, serialize_into};
use crate::{Error, Result};
use memmap2::MmapMut;
use std::fs::File;
use std::io::{BufReader, Write};
use tempfile::tempfile;

pub struct PackedRTreeWriter {
    mmap: MmapMut,
    tree: PackedRTree,
    sorted_leaf_nodes: Vec<Node>,
    temp_index_file: File,
    features_added: u64,
}

impl PackedRTreeWriter {
    pub fn new(leaf_node_count: u64) -> Result<Self> {
        let temp_index_file = tempfile()?;
        let tree = PackedRTree::new(leaf_node_count);
        let size = tree.index_size();
        temp_index_file.set_len(size)?;

        let mmap = unsafe { MmapMut::map_mut(&temp_index_file)? };

        Ok(Self {
            mmap,
            tree,
            sorted_leaf_nodes: vec![],
            temp_index_file,
            features_added: 0,
        })
    }

    pub fn push_leaf(&mut self, leaf: Node) -> Result<()> {
        self.features_added += 1;
        self.sorted_leaf_nodes.push(leaf);
        Ok(())
    }

    pub fn write<W: Write>(mut self, mut writer: W) -> Result<()> {
        if self.features_added != self.tree.num_leaf_nodes {
            return Err(Error::FeatureCountMismatch {
                expected: self.tree.num_leaf_nodes,
                found: self.features_added,
            });
        }

        if self.tree.num_leaf_nodes == 0 {
            return Ok(());
        }

        // PERF: don't write this to vec
        let mut nodes_for_this_level = self.sorted_leaf_nodes.to_vec();
        let mut byte_ranges = self.tree.byte_ranges_by_level();
        byte_ranges.reverse();
        for byte_range_of_level in byte_ranges {
            let mut writer = &mut self.mmap[byte_range_of_level.clone()];
            for node in nodes_for_this_level {
                serialize_into(&mut writer, &node)?;
            }
            nodes_for_this_level = {
                let prev_level: &[u8] = &self.mmap[byte_range_of_level.clone()];
                prev_level
                    .chunks(Node::serialized_size() * BRANCHING_FACTOR as usize)
                    .map(|children_bytes| {
                        let mut parent = Node::empty_inner_node();
                        for child_bytes in children_bytes.chunks(Node::serialized_size()) {
                            let child: Node = deserialize_from(child_bytes)?;
                            parent.bounds.extend(&child.bounds);
                        }
                        Ok(parent)
                    })
                    .collect::<Result<Vec<Node>>>()?
            };
        }

        self.mmap.flush()?;

        // REVIEW: Do we need to ensure mmap has synced?
        // REVIEW: vs copying from memmap?
        std::io::copy(&mut BufReader::new(self.temp_index_file), &mut writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::Bounds;
    use crate::writer::FeatureLocation;
    use crate::{wkt, LngLat};

    #[test]
    fn write_empty() {
        let mut output: Vec<u8> = vec![];
        let tree = PackedRTreeWriter::new(0).unwrap();
        tree.write(&mut output).unwrap();

        let expected: Vec<u8> = vec![];
        assert_eq!(output, expected);
    }

    #[test]
    fn write_single() {
        let mut output: Vec<u8> = vec![];
        let mut tree = PackedRTreeWriter::new(1).unwrap();
        tree.push_leaf(Node {
            bounds: Bounds::from_corners(&LngLat::unscaled(1, 2), &LngLat::unscaled(3, 4)),
            offset: FeatureLocation {
                page_starting_offset: 60,
                feature_offset: 8,
            },
        })
        .unwrap();
        tree.write(&mut output).unwrap();

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            // Bounding Box
            1, 0, 0, 0, //  1i32 as LE bytes
            2, 0, 0, 0, //  2i32 as LE bytes
            3, 0, 0, 0, //  3i32 as LE bytes
            4, 0, 0, 0, //  4i32 as LE bytes

            // FeatureLocation
            60, 0, 0, 0, 0, 0, 0, 0, // Page offset as LE bytes
            8, 0, 0, 0,              // Feature offset as LE bytes
        ];

        assert_eq!(output, expected);
    }

    #[test]
    fn write_multiple_layers() {
        let mut output: Vec<u8> = vec![];
        let mut tree = PackedRTreeWriter::new(17).unwrap();

        for offset in 0..17 {
            let bounds = Bounds::from_corners(
                &LngLat::degrees(offset as f64, offset as f64),
                &LngLat::degrees(offset as f64 * 2.0, offset as f64 * 2.0),
            );
            tree.push_leaf(Node {
                bounds,
                // 10 features per page
                offset: FeatureLocation {
                    page_starting_offset: offset / 10, // fake page size
                    feature_offset: offset as u32 % 10,
                },
            })
            .unwrap();
        }

        tree.write(&mut output).unwrap();

        let mut reader = output.as_slice();

        // root
        let root: Node = deserialize_from(&mut reader).unwrap();
        assert_eq!(root.bounds, wkt!(RECT(0 0,32 32)));

        let level_1 = vec![
            deserialize_from::<_, Node>(&mut reader).unwrap(),
            deserialize_from::<_, Node>(&mut reader).unwrap(),
        ];
        assert_eq!(level_1[0].bounds, wkt!(RECT(0 0, 30 30)));
        assert_eq!(level_1[1].bounds, wkt!(RECT(16 16, 32 32)));

        let mut level_2 = vec![];
        for _ in 0..17 {
            level_2.push(deserialize_from::<_, Node>(&mut reader).unwrap());
        }

        assert_eq!(level_2[0].bounds, wkt!(RECT(0 0, 0 0)));
        assert_eq!(level_2[16].bounds, wkt!(RECT(16 16, 32 32)));
    }
}
