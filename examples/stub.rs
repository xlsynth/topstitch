// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{EmitOptions, ParserConfig, Source, VerilogLibrary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Path to the "examples" folder

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let input = examples.join("input");

    // Parse the Verilog sources. Note that multiple sources can be specified.
    let sources = [
        Source::file(input.join("pack.sv").to_str().unwrap()),
        Source::file(input.join("block.sv").to_str().unwrap()),
    ];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        ..Default::default()
    })?;
    let block = library.module("block")?.ok_or("Missing block module")?;

    // Parameterization is optional; it's shown here to illustrate the feature. The
    // parameterize() function returns a new ModDef with the given parameter values.
    // Unspecified parameters will use their default values.
    let block_parameterized = block.parameterize(&[("N", 32)]);

    // Create a stub for the parameterized block. Try replacing
    // "block_parameterized" with "block" - it will still work, using default
    // parameter values.
    let stub = block_parameterized.stub("stub");

    let a = stub.get_port("a");
    let b_array = stub.get_port("b").subdivide(2);

    a.connect(&b_array[0]);
    b_array[1].tieoff(0);

    stub.get_port("c").connect(&stub.get_port("d"));

    // Emit the final Verilog code

    let output_dir = examples.join("output");
    std::fs::create_dir_all(&output_dir).expect("should be possible to create output dir");
    let output_file = output_dir.join("stub.sv");
    stub.emit_to_file(&output_file, EmitOptions::default());
    eprintln!("Emitted to output file: {}", output_file.display());
    Ok(())
}
