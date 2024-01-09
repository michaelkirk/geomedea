mod reader;
mod writer;

pub use reader::http::PackedRTreeHttpReader;
pub use reader::PackedRTreeReader;
use std::cell::OnceCell;
pub use writer::PackedRTreeWriter;

use crate::writer::FeatureLocation;
use crate::Bounds;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::fmt::Debug;
use std::ops::Range;

// TODO: make configurable and store in PackedRTree
const BRANCHING_FACTOR: u64 = 16;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    bounds: Bounds,
    offset: FeatureLocation,
}

impl Node {
    pub(crate) fn leaf_node(bounds: Bounds, offset: FeatureLocation) -> Self {
        Self { bounds, offset }
    }

    pub fn serialized_size() -> usize {
        let node_size = 28;
        #[cfg(debug_assertions)]
        {
            let computed_node_size =
                crate::serialized_size(&Self::empty_inner_node()).unwrap() as usize;
            assert_eq!(computed_node_size, node_size);
        }
        node_size
    }

    pub(crate) fn empty_inner_node() -> Self {
        // TODO: `offset` is unused for inner nodes, should we get rid of it?
        // We can infer based on position wether something is an inner vs outter
        // node, but it might not be worth much... might be worth a few percent.
        Self {
            bounds: Bounds::empty(),
            offset: FeatureLocation {
                page_starting_offset: 0,
                feature_offset: 0,
            },
        }
    }
}

pub struct PackedRTree {
    num_leaf_nodes: u64,
    node_ranges_by_level: OnceCell<Vec<Range<u64>>>,
}

impl PackedRTree {
    pub(crate) fn new(num_leaf_nodes: u64) -> Self {
        Self {
            num_leaf_nodes,
            node_ranges_by_level: OnceCell::new(),
        }
    }

    pub fn index_size(&self) -> u64 {
        self.node_count() * Node::serialized_size() as u64
    }

    fn nodes_per_level(&self) -> Vec<u64> {
        if self.num_leaf_nodes == 0 {
            return vec![];
        }

        let mut levels = vec![];
        levels.push(self.num_leaf_nodes);

        let mut level_nodes = self.num_leaf_nodes;
        while level_nodes > 1 {
            let full_parents = level_nodes / BRANCHING_FACTOR;
            if full_parents * BRANCHING_FACTOR == level_nodes {
                level_nodes = full_parents;
            } else {
                level_nodes = full_parents + 1;
            }
            levels.push(level_nodes);
        }
        levels.reverse();
        levels
    }

    pub fn node_count(&self) -> u64 {
        self.nodes_per_level().iter().sum()
    }

    pub fn byte_ranges_by_level(&self) -> Vec<Range<usize>> {
        let node_size = Node::serialized_size();
        self.node_ranges_by_level()
            .iter()
            .map(|node_range| {
                (node_size * node_range.start as usize)..(node_size * node_range.end as usize)
            })
            .collect::<Vec<_>>()
    }

    /// PERF: This is a hot one. Cache it?
    /// The range of nodes belonging to each level (in _nodes_, not bytes)
    pub fn node_ranges_by_level(&self) -> &[Range<u64>] {
        self.node_ranges_by_level
            .get_or_init(|| self._node_ranges_by_level())
    }

    pub fn _node_ranges_by_level(&self) -> Vec<Range<u64>> {
        let levels = self.nodes_per_level();
        let mut total_offset = 0;
        let mut offsets = vec![];
        for level_width in levels {
            offsets.push(total_offset..total_offset + level_width);
            total_offset += level_width;
        }
        offsets
    }

    /// Given a node idx, returns the indices of its children
    pub fn children_range(&self, node_idx: u64) -> Option<Range<u64>> {
        let ranges = self.node_ranges_by_level();
        let mut range_iter = ranges.iter();

        let mut parent_position_in_level = None;
        for this_level in &mut range_iter {
            if let Some(position_in_level) = this_level.clone().position(|idx| idx == node_idx) {
                parent_position_in_level = Some(position_in_level as u64);
                break;
            };
        }

        let parent_position_in_level = parent_position_in_level?;
        let child_level = range_iter.next()?;

        let children_start = child_level.start + parent_position_in_level * BRANCHING_FACTOR;

        Some(children_start..min(children_start + BRANCHING_FACTOR, child_level.end))
    }

    fn level_for_node_idx(&self, node_idx: u64) -> usize {
        debug_assert!(
            node_idx < self.node_count(),
            "requested level for node #{node_idx} when there are only {} nodes",
            self.node_count()
        );
        let levels = self.node_ranges_by_level();
        let level_idx = levels
            .iter()
            .enumerate()
            .find(|(_level, range)| range.contains(&node_idx))
            .expect("already verified node_idx was within *some* node range")
            .0;

        debug_assert!(
            !levels.is_empty(),
            "already verified node_idx was within *some* node range, thus the tree is non-empty"
        );
        levels.len() - 1 - level_idx
    }

    fn is_leaf_node(&self, node_idx: u64) -> bool {
        let levels = self.node_ranges_by_level();
        let Some(features) = levels.last() else {
            debug_assert!(false, "Doesnt make sense to ask about an empty tree");
            return false;
        };

        node_idx >= features.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_leaf_node() {
        let tree = PackedRTree::new(257);
        assert!(!tree.is_leaf_node(0));
        assert!(!tree.is_leaf_node(3));
        assert!(!tree.is_leaf_node(19));
        assert!(tree.is_leaf_node(20));
        assert!(tree.is_leaf_node(277));
    }

    #[test]
    fn nodes_per_level() {
        assert_eq!(Vec::<u64>::new(), PackedRTree::new(0).nodes_per_level());
        assert_eq!(vec![1], PackedRTree::new(1).nodes_per_level());
        assert_eq!(vec![1, 2], PackedRTree::new(2).nodes_per_level());
        assert_eq!(vec![1, 4], PackedRTree::new(4).nodes_per_level());
        assert_eq!(vec![1, 15], PackedRTree::new(15).nodes_per_level());
        assert_eq!(vec![1, 16], PackedRTree::new(16).nodes_per_level());
        assert_eq!(vec![1, 2, 17], PackedRTree::new(17).nodes_per_level());
        assert_eq!(vec![1, 2, 32], PackedRTree::new(32).nodes_per_level());
        assert_eq!(vec![1, 3, 33], PackedRTree::new(33).nodes_per_level());
        assert_eq!(vec![1, 16, 255], PackedRTree::new(255).nodes_per_level());
        assert_eq!(vec![1, 16, 256], PackedRTree::new(256).nodes_per_level());
        assert_eq!(vec![1, 2, 17, 257], PackedRTree::new(257).nodes_per_level());
    }

    #[test]
    fn byte_ranges_by_level() {
        assert_eq!(vec![(0..28)], PackedRTree::new(1).byte_ranges_by_level());
        assert_eq!(
            vec![(0..28), (28..112)],
            PackedRTree::new(3).byte_ranges_by_level()
        );
    }
    #[test]
    fn node_ranges_by_level() {
        assert_eq!(vec![(0..1)], PackedRTree::new(1).node_ranges_by_level());
        assert_eq!(
            vec![(0..1), (1..3)],
            PackedRTree::new(2).node_ranges_by_level()
        );
        assert_eq!(
            vec![(0..1), (1..17)],
            PackedRTree::new(16).node_ranges_by_level()
        );
        assert_eq!(
            vec![(0..1), (1..3), (3..20)],
            PackedRTree::new(17).node_ranges_by_level()
        );
        assert_eq!(
            vec![(0..1), (1..17), (17..273)],
            PackedRTree::new(256).node_ranges_by_level()
        );
        assert_eq!(
            vec![(0..1), (1..3), (3..20), (20..277)],
            PackedRTree::new(257).node_ranges_by_level()
        );
    }

    #[test]
    fn node_count() {
        assert_eq!(1, PackedRTree::new(1).node_count());
        assert_eq!(2 + 1, PackedRTree::new(2).node_count());
        assert_eq!(16 + 1, PackedRTree::new(16).node_count());
        assert_eq!(256 + 16 + 1, PackedRTree::new(256).node_count());
        assert_eq!(257 + 17 + 2 + 1, PackedRTree::new(257).node_count());
    }

    #[test]
    fn level_for_node_idx() {
        let tree = PackedRTree::new(250);
        assert_eq!(tree.level_for_node_idx(17), 0);
        assert_eq!(tree.level_for_node_idx(266), 0);
        assert_eq!(tree.level_for_node_idx(16), 1);
        assert_eq!(tree.level_for_node_idx(1), 1);
        assert_eq!(tree.level_for_node_idx(0), 2);
    }

    mod children_range {
        use super::*;

        #[test]
        fn empty() {
            assert_eq!(None, PackedRTree::new(0).children_range(0));
            assert_eq!(None, PackedRTree::new(0).children_range(5));
        }

        #[test]
        fn single_node() {
            assert_eq!(None, PackedRTree::new(1).children_range(0));
        }

        #[test]
        fn two_levels() {
            assert_eq!(Some(1..3), PackedRTree::new(2).children_range(0));
            assert_eq!(Some(1..4), PackedRTree::new(3).children_range(0));
            assert_eq!(None, PackedRTree::new(3).children_range(2));
            assert_eq!(None, PackedRTree::new(3).children_range(5));
        }

        #[test]
        fn three_levels() {
            assert_eq!(Some(1..3), PackedRTree::new(17).children_range(0));
            assert_eq!(Some(3..19), PackedRTree::new(17).children_range(1));
            assert_eq!(Some(19..20), PackedRTree::new(17).children_range(2));
        }
    }
}

/*
use crate::Bounds;

struct Node {
    bounds: Bounds,
    page: u64,
}

struct PackedRTree {
    branching_factor: usize,
    nodes: Vec<Node>,
}

/// Allocate space for tree
/// Insert leaves in order, compute parent bbox's after everything is inserted?
///
impl PackedRTree {}
 */
