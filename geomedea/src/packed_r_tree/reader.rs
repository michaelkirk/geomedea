use crate::bounds::Bounds;
use crate::packed_r_tree::{Node, PackedRTree};
use crate::FeatureLocation;
use crate::{deserialize_from, Result};
use std::collections::VecDeque;
use std::io::Read;
use std::ops::Range;

pub struct PackedRTreeReader<R> {
    read: R,
    tree: PackedRTree,
    node_position: u64,
}

impl<R: Read> PackedRTreeReader<R> {
    pub(crate) fn new(num_leaf_nodes: u64, read: R) -> Self {
        let tree = PackedRTree::new(num_leaf_nodes);
        Self {
            read,
            tree,
            node_position: 0,
        }
    }

    pub fn select_bbox(mut self, bbox: &Bounds) -> Result<Vec<FeatureLocation>> {
        if self.tree.num_leaf_nodes == 0 {
            return Ok(vec![]);
        }

        let mut results = vec![];
        let mut queue = VecDeque::new();
        queue.push_back(0..1);

        while let Some(node_range) = queue.pop_front() {
            for (node_idx, node) in self.read_node_range(node_range)? {
                if !node.bounds.intersects(bbox) {
                    continue;
                }
                if self.tree.is_leaf_node(node_idx) {
                    results.push(node.offset);
                } else if let Some(children) = self.tree.children_range(node_idx) {
                    // TODO: merge
                    queue.push_back(children);
                }
            }
        }

        Ok(results)
    }

    fn read_node_range(&mut self, nodes: Range<u64>) -> Result<Vec<(u64, Node)>> {
        assert!(self.node_position <= nodes.start);
        assert!(!nodes.is_empty());

        while self.node_position < nodes.start {
            let _: Node = deserialize_from(&mut self.read)?;
            self.node_position += 1;
        }

        assert!(nodes.end > nodes.start);
        let len = (nodes.end - nodes.start) as usize;
        let mut results = Vec::with_capacity(len);
        for node_idx in nodes.clone() {
            let node = deserialize_from(&mut self.read)?;
            self.node_position += 1;
            results.push((node_idx, node));
        }

        assert_eq!(self.node_position, nodes.end);
        Ok(results)
    }
}

pub(crate) mod http {
    use crate::asyncio::AsyncReadExt;
    use crate::packed_r_tree::{Node, PackedRTree};
    use crate::FeatureLocation;
    use crate::Result;
    use crate::{deserialize_from, Bounds};
    use std::collections::VecDeque;
    use std::ops::Range;
    use streaming_http_range_client::{HttpClient, HttpRange};

    pub struct PackedRTreeHttpReader {
        http_client: HttpClient,
        index_starting_byte: u64,
        tree: PackedRTree,
    }

    impl PackedRTreeHttpReader {
        pub(crate) fn new(
            feature_count: u64,
            http_client: HttpClient,
            index_starting_offset: u64,
        ) -> Self {
            let tree = PackedRTree::new(feature_count);
            Self {
                http_client,
                index_starting_byte: index_starting_offset,
                tree,
            }
        }

        pub async fn select_bbox(&mut self, bbox: &Bounds) -> Result<Vec<FeatureLocation>> {
            trace!("select_bbox {bbox:?}");
            if self.tree.num_leaf_nodes == 0 {
                return Ok(vec![]);
            }

            let mut results = vec![];
            let mut queue = VecDeque::new();
            queue.push_back(0..1);

            while let Some(node_range) = queue.pop_front() {
                let level = self.tree.level_for_node_idx(node_range.start);
                trace!("next node_range {node_range:?} (level {level})");
                // REVIEW: why return node_idx? Can't we infer it from node_range?
                for (node_idx, node) in self.read_node_range(node_range).await? {
                    if !node.bounds.intersects(bbox) {
                        continue;
                    }
                    if self.tree.is_leaf_node(node_idx) {
                        results.push(node.offset);
                    } else if let Some(children) = self.tree.children_range(node_idx) {
                        let Some(tail) = queue.back_mut() else {
                            let level = self.tree.level_for_node_idx(children.start);
                            debug!(
                                "pushing children onto empty queue: {children:?} (level {level})"
                            );
                            queue.push_back(children);
                            continue;
                        };

                        let tail_level = self.tree.level_for_node_idx(tail.start);
                        debug_assert_eq!(tail_level, self.tree.level_for_node_idx(tail.end));

                        let children_level = self.tree.level_for_node_idx(children.start);
                        debug_assert_eq!(
                            children_level,
                            self.tree.level_for_node_idx(children.end - 1)
                        );

                        if tail_level != children_level {
                            debug!("pushing new level {children_level} for children: {children:?}, since queue tail has level {tail_level}");
                            queue.push_back(children);
                            continue;
                        }

                        // TODO: do something less arbitrary
                        let combine_request_threshold = 16_000;
                        let combine_request_node_threshold =
                            combine_request_threshold / Node::serialized_size();
                        if tail.end + combine_request_node_threshold as u64 > children.start {
                            trace!("merging children: {children:?} with nearby existing range {tail:?}");
                            debug_assert!(
                                children.start >= tail.end,
                                "Failed: {} > {}",
                                children.start,
                                tail.end
                            );
                            tail.end = children.end;
                            continue;
                        }

                        debug!("pushing new node {children:?} range rather than merging with distance node range {tail:?}");
                        queue.push_back(children);
                    }
                }
            }

            Ok(results)
        }

        pub(crate) fn into_http_client(self) -> HttpClient {
            self.http_client
        }

        async fn read_node_range(&mut self, node_range: Range<u64>) -> Result<Vec<(u64, Node)>> {
            let start_byte =
                self.index_starting_byte + node_range.start * Node::serialized_size() as u64;
            let end_byte =
                self.index_starting_byte + node_range.end * Node::serialized_size() as u64;
            let range = HttpRange::Range(start_byte..end_byte);
            self.http_client.seek_to_range(range).await?;

            let node_range_len = (node_range.end - node_range.start) as usize;
            let mut nodes = Vec::with_capacity(node_range_len);
            for node_id in node_range {
                let mut node_bytes = vec![0u8; Node::serialized_size()];
                self.http_client.read_exact(&mut node_bytes).await?;
                let node: Node = deserialize_from(&*node_bytes)?;
                nodes.push((node_id, node))
            }

            Ok(nodes)
        }
        pub fn tree(&self) -> &PackedRTree {
            &self.tree
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::tests::example_index;
        use crate::packed_r_tree::PackedRTreeHttpReader;
        use crate::{wkt, FeatureLocation};
        use streaming_http_range_client::HttpClient;

        #[tokio::test]
        async fn http_search() {
            let index_bytes = example_index();
            let mut http_client = HttpClient::test_client(&index_bytes);
            // avoid some dumb precondition of HttpClient
            http_client.set_range(0..1).await.unwrap();

            // Search
            let mut reader = PackedRTreeHttpReader::new(4, http_client, 0);
            let locations = reader
                .select_bbox(&wkt!(RECT(0.5 0.5,0.75 0.75)))
                .await
                .unwrap();
            assert_eq!(
                locations,
                vec![FeatureLocation {
                    page_starting_offset: 0,
                    feature_offset: 0
                }]
            );

            let mut http_client = HttpClient::test_client(&index_bytes);
            // avoid some dumb precondition of HttpClient
            http_client.set_range(0..1).await.unwrap();
            let mut reader = PackedRTreeHttpReader::new(4, http_client, 0);
            let locations = reader
                .select_bbox(&wkt!(RECT(1.5 1.5,2.0 2.0)))
                .await
                .unwrap();
            assert_eq!(
                locations,
                vec![
                    FeatureLocation {
                        page_starting_offset: 0,
                        feature_offset: 1
                    },
                    FeatureLocation {
                        page_starting_offset: 10,
                        feature_offset: 0
                    }
                ]
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bounded;
    use crate::packed_r_tree::{Node, PackedRTreeWriter};
    use crate::{wkt, FeatureLocation};

    pub(crate) fn example_index() -> Vec<u8> {
        let mut writer = PackedRTreeWriter::new(4).unwrap();
        writer
            .push_leaf(Node {
                bounds: wkt!(RECT(0 0,1 1)),
                offset: FeatureLocation {
                    page_starting_offset: 0,
                    feature_offset: 0,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: wkt!(RECT(1 1,2 2)),
                offset: FeatureLocation {
                    page_starting_offset: 0,
                    feature_offset: 1,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: wkt!(RECT(2 2,3 3)),
                offset: FeatureLocation {
                    page_starting_offset: 10,
                    feature_offset: 0,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: wkt!(RECT(3 3,4 4)),
                offset: FeatureLocation {
                    page_starting_offset: 10,
                    feature_offset: 1,
                },
            })
            .unwrap();
        let mut output = vec![];
        writer.write(&mut output).unwrap();
        output
    }

    #[test]
    fn search() {
        let index_bytes = example_index();

        // Search
        let reader = PackedRTreeReader::new(4, index_bytes.as_slice());
        let locations = reader.select_bbox(&wkt!(RECT(0.5 0.5,0.75 0.75))).unwrap();
        assert_eq!(
            locations,
            vec![FeatureLocation {
                page_starting_offset: 0,
                feature_offset: 0
            }]
        );

        let reader = PackedRTreeReader::new(4, index_bytes.as_slice());
        let locations = reader.select_bbox(&wkt!(RECT(1.5 1.5,2.0 2.0))).unwrap();
        assert_eq!(
            locations,
            vec![
                FeatureLocation {
                    page_starting_offset: 0,
                    feature_offset: 1
                },
                FeatureLocation {
                    page_starting_offset: 10,
                    feature_offset: 0
                }
            ]
        );
    }

    // not currently implemented - I'm not sure if we should.
    #[ignore]
    #[test]
    fn span_idl() {
        let a = wkt!(POINT(179.0 50));
        let b = wkt!(POINT(-179.0 50));
        let too_far_west = wkt!(POINT(170.0 50));
        let too_far_east = wkt!(POINT(-170.0 50));

        let mut writer = PackedRTreeWriter::new(4).unwrap();

        writer
            .push_leaf(Node {
                bounds: a.bounds(),
                offset: FeatureLocation {
                    page_starting_offset: 0,
                    feature_offset: 0,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: b.bounds(),
                offset: FeatureLocation {
                    page_starting_offset: 10,
                    feature_offset: 0,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: too_far_west.bounds(),
                offset: FeatureLocation {
                    page_starting_offset: 20,
                    feature_offset: 0,
                },
            })
            .unwrap();
        writer
            .push_leaf(Node {
                bounds: too_far_east.bounds(),
                offset: FeatureLocation {
                    page_starting_offset: 30,
                    feature_offset: 0,
                },
            })
            .unwrap();

        let mut output = vec![];
        writer.write(&mut output).unwrap();

        // Search
        let reader = PackedRTreeReader::new(4, output.as_slice());
        let locations: Vec<_> = reader
            .select_bbox(&wkt!(RECT(179 49,-179 51)))
            .unwrap()
            .iter()
            .map(|l| l.page_starting_offset)
            .collect();

        assert_eq!(locations, vec![0, 10]);
    }
}
