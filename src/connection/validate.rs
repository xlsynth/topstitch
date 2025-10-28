// SPDX-License-Identifier: Apache-2.0

use crate::connection::port_slice::PortSliceConnections;
use crate::io::IO;

/// Asserts that a collection of non-overlapping connection chunks fully
/// covers a port without gaps.
///
/// Expectations and preconditions:
/// - `connections` must be the result of tracing and non-overlap merging for a
///   single port, where each `PortSliceConnections` entry represents a
///   contiguous chunk of bits for that port.
/// - Chunks are expected to be sorted in descending bit order (highest MSB
///   first); callers are responsible for ordering and for removing empties.
/// - `io` provides the full declared width of the port being checked.
///
/// Behavior:
/// - If `connections` is empty, or the first/last chunk is empty, the function
///   panics with a descriptive message.
/// - It panics if there is a gap at the top (between the port MSB and the MSB
///   of the first chunk), at the bottom (between the LSB of the last chunk and
///   0), or between any adjacent chunks.
/// - Panic messages include the debug path in `debug_str` and the precise
///   bit-range that is missing in the form `[msb:lsb]` or `[i]`.
pub fn check_for_gaps(connections: &[PortSliceConnections], io: &IO, debug_str: &str) {
    let first = connections
        .first()
        .unwrap_or_else(|| panic!("{debug_str} is unconnected"));
    let last = connections
        .last()
        .unwrap_or_else(|| panic!("{debug_str} is unconnected"));

    assert!(
        !first.is_empty(),
        "Invalid connection found for {debug_str}"
    );
    assert!(!last.is_empty(), "Invalid connection found for {debug_str}");

    // make sure there is no gap at the top
    let actual_msb = first[0].this.msb;
    let gap = (io.width() - 1) - actual_msb;
    assert!(
        gap == 0,
        "{debug_str}{} is unconnected",
        slice_fmt(io.width() - 1, actual_msb + 1)
    );

    // make sure there is no gap at the bottom
    let actual_lsb = last[0].this.lsb;
    let gap = actual_lsb;
    assert!(
        gap == 0,
        "{debug_str}{} is unconnected",
        slice_fmt(actual_lsb - 1, 0)
    );

    // make sure there are no gaps between chunks
    for i in 0..connections.len() - 1 {
        let chunk_i_lsb = connections[i][0].this.lsb;
        let chunk_i_plus_1_msb = connections[i + 1][0].this.msb;
        assert!(
            chunk_i_lsb == (chunk_i_plus_1_msb + 1),
            "{debug_str}{} is unconnected",
            slice_fmt(chunk_i_lsb - 1, chunk_i_plus_1_msb + 1),
        );
    }
}

/// Formats a bit-range for diagnostic messages. If `msb == lsb`, formats as
/// `[i]`; otherwise formats as `[msb:lsb]`.
fn slice_fmt(msb: usize, lsb: usize) -> String {
    if msb == lsb {
        format!("[{}]", msb)
    } else {
        format!("[{}:{}]", msb, lsb)
    }
}
