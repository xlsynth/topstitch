// SPDX-License-Identifier: Apache-2.0

use regex::Captures;

pub fn concat_captures(captures: &Captures, sep: &str) -> String {
    captures
        .iter()
        .skip(1)
        .filter_map(|m| m.map(|m| m.as_str().to_string()))
        .collect::<Vec<String>>()
        .join(sep)
}
