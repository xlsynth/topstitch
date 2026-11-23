// SPDX-License-Identifier: Apache-2.0

/// A reduction splits by element count. A mux splits at binary address boundaries.
#[derive(Clone, Copy)]
pub(crate) enum Tree {
    Reduction,
    Select { index_width: usize },
}

/// Builds only populated subtrees. The join receives the selector bit, low child and high child.
pub(crate) fn fold_tree<T: Clone>(
    leaves: &[T],
    shape: Tree,
    fallback: Option<&T>,
    join: &mut impl FnMut(usize, T, T) -> T,
) -> T {
    /// Splits a populated span at the next address bit or the midpoint of a reduction.
    fn populated<T: Clone>(
        leaves: &[T],
        levels: usize,
        mux: bool,
        fill: Option<&T>,
        join: &mut impl FnMut(usize, T, T) -> T,
    ) -> T {
        if leaves.is_empty() {
            return fill.expect("missing mux fallback").clone();
        }
        if (!mux && leaves.len() == 1) || levels == 0 {
            return leaves[0].clone();
        }
        let split = if mux {
            (1usize << (levels - 1)).min(leaves.len())
        } else {
            leaves.len() / 2
        };
        let (low, high) = leaves.split_at(split);
        let a = populated(low, levels - 1, mux, fill, join);
        let b = populated(high, levels - 1, mux, fill, join);
        join(levels - 1, a, b)
    }
    if leaves.is_empty() {
        return fallback.expect("empty tree needs a fallback").clone();
    }
    let levels = usize::BITS as usize - (leaves.len() - 1).leading_zeros() as usize;
    let mut value = populated(
        leaves,
        levels,
        matches!(shape, Tree::Select { .. }),
        fallback,
        join,
    );
    if let Tree::Select { index_width } = shape {
        for bit in levels..index_width {
            value = join(bit, value, fallback.expect("missing high mux arms").clone());
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reductions_keep_leaf_depths_within_one_level() {
        for width in 2usize..=129 {
            let leaves = vec![(0, 0); width];
            let mut count = 0;
            let (min, max) = fold_tree(&leaves, Tree::Reduction, None, &mut |_, lo, hi| {
                count += 1;
                (1 + lo.0.min(hi.0), 1 + lo.1.max(hi.1))
            });
            assert_eq!(count, width - 1);
            assert_eq!(max, width.ilog2() + u32::from(!width.is_power_of_two()));
            assert!(max - min <= 1);
        }
    }

    #[test]
    fn sparse_tables_grow_with_index_width_and_occupied_addresses() {
        // A single populated arm needs one mux per selector bit, even for a 128-bit address.
        let mut controls = Vec::new();
        let value = fold_tree(
            &[7],
            Tree::Select { index_width: 128 },
            Some(&0),
            &mut |bit, lo, hi| {
                controls.push(bit);
                assert_eq!(hi, 0);
                lo
            },
        );
        assert_eq!(value, 7);
        assert_eq!(controls, (0..128).collect::<Vec<_>>());
    }
}
