// SPDX-License-Identifier: Apache-2.0

use crate::connection::port_slice::PortSliceConnections;
use crate::io::IO;

impl PortSliceConnections {
    /// Resolves a whole port into contiguous, fully covered chunks, most significant first.
    pub(crate) fn resolved_chunks(&self, io: &IO, name: &str) -> Result<Vec<Self>, String> {
        let mut chunks = self.try_trace()?.make_non_overlapping();
        chunks.retain(|chunk| !chunk.is_empty());
        chunks.reverse(); // make_non_overlapping visits the intervals from low to high.
        if chunks.is_empty() {
            return Err(format!("{name} is unconnected"));
        }

        let mut end = io.width();
        for chunk in &chunks {
            let slice = &chunk[0].this;
            if slice.msb + 1 < end {
                return Err(format!(
                    "{name}{} is unconnected",
                    slice_fmt(end - 1, slice.msb + 1)
                ));
            }
            end = slice.lsb;
        }
        if end > 0 {
            return Err(format!("{name}{} is unconnected", slice_fmt(end - 1, 0)));
        }
        Ok(chunks)
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
