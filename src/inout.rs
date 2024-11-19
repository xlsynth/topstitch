// SPDX-License-Identifier: Apache-2.0

// TODO(sherbst) 11/19/24: Replace with a VAST API call.

pub const INOUT_MARKER: &str = "_INOUT_RENAME";

pub fn rename_inout(text: String) -> String {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    for line in &mut lines {
        let inout_rename = line.contains(INOUT_MARKER);
        if !inout_rename {
            continue;
        }

        let input_port = line.trim_start().starts_with("input");

        if input_port {
            *line = line.replacen("input", "inout", 1);
        }

        *line = line.replace(INOUT_MARKER, "");
    }
    lines.join("\n")
}
