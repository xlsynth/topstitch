// SPDX-License-Identifier: Apache-2.0

/// An entry covering an inclusive range of bits on one port.
pub(super) struct Interval<T> {
    pub(super) lsb: usize,
    pub(super) msb: usize,
    pub(super) value: T,
}

impl<T> Interval<T> {
    /// Number of bits covered by this entry.
    pub(super) fn width(&self) -> usize {
        self.msb - self.lsb + 1
    }
}

/// A query-local index of possibly overlapping intervals, ordered by LSB.
pub(super) struct IntervalIndex<T> {
    entries: Vec<Interval<T>>,
    max_end: Vec<usize>,
}

impl<T> IntervalIndex<T> {
    /// Sorts entries and records each subtree's largest MSB at its median.
    pub(super) fn new(mut entries: Vec<Interval<T>>) -> Self {
        entries.sort_by_key(|entry| entry.lsb);
        let mut max_end = vec![0; entries.len()];
        Self::index(&entries, &mut max_end);
        Self { entries, max_end }
    }

    /// Returns intersecting entries in increasing LSB order, retaining ties and aliases.
    pub(super) fn overlapping(&self, lo: usize, hi: usize) -> Vec<&Interval<T>> {
        let mut result = Vec::new();
        self.collect(0, self.entries.len(), lo, hi, &mut result);
        result
    }

    /// Builds the subtree maxima bottom-up.
    fn index(entries: &[Interval<T>], ends: &mut [usize]) -> usize {
        if entries.is_empty() {
            return 0;
        }
        let mid = entries.len() / 2;
        let (left, rest) = ends.split_at_mut(mid);
        let (center, right) = rest.split_first_mut().unwrap();
        *center = entries[mid]
            .msb
            .max(Self::index(&entries[..mid], left))
            .max(Self::index(&entries[mid + 1..], right));
        *center
    }

    /// Collects matches in this subtree and counts visited nodes for the scaling regression.
    fn collect<'a>(
        &'a self,
        start: usize,
        end: usize,
        lo: usize,
        hi: usize,
        out: &mut Vec<&'a Interval<T>>,
    ) -> usize {
        if start == end {
            return 0;
        }
        let mid = start + (end - start) / 2;
        if self.max_end[mid] < lo || self.entries[start].lsb > hi {
            return 1;
        }
        let left = self.collect(start, mid, lo, hi, out);
        let entry = &self.entries[mid];
        if entry.lsb <= hi && entry.msb >= lo {
            out.push(entry);
        }
        1 + left + self.collect(mid + 1, end, lo, hi, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_tap_does_not_force_a_linear_scan_for_one_bit() {
        let mut entries = vec![Interval {
            lsb: 0,
            msb: 4095,
            value: (),
        }];
        entries.extend((0..4096).map(|bit| Interval {
            lsb: bit,
            msb: bit,
            value: (),
        }));
        let index = IntervalIndex::new(entries);
        for bit in [0, 1, 2048, 4095] {
            let mut overlaps = Vec::new();
            let inspected = index.collect(0, index.entries.len(), bit, bit, &mut overlaps);
            assert_eq!(overlaps.len(), 2);
            assert!(
                inspected < 64,
                "inspected {inspected} nodes to find two mappings"
            );
        }
    }
}
