// SPDX-License-Identifier: Apache-2.0

use crate::ParserConfig;
use slang_rs::Package;
use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

pub fn extract_packages_from_verilog_file(
    verilog: &Path,
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    extract_packages_from_verilog_files(&[verilog], ignore_unknown_modules)
}

pub fn extract_packages_from_verilog_files(
    verilog: &[&Path],
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let cfg = ParserConfig {
        sources: &verilog
            .iter()
            .map(|path| path.to_str().unwrap())
            .collect::<Vec<_>>(),
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

pub fn extract_packages_from_verilog(
    verilog: impl AsRef<str>,
    ignore_unknown_modules: bool,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    let verilog = slang_rs::str2tmpfile(verilog.as_ref()).unwrap();

    let cfg = ParserConfig {
        sources: &[verilog.path().to_str().unwrap()],
        ignore_unknown_modules,
        ..Default::default()
    };

    extract_packages_with_config(&cfg)
}

pub fn extract_packages_with_config(
    cfg: &ParserConfig,
) -> Result<HashMap<String, Package>, Box<dyn Error>> {
    slang_rs::extract_packages(&cfg.to_slang_config())
}
