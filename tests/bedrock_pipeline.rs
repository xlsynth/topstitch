// SPDX-License-Identifier: Apache-2.0

use topstitch::{ParserConfig, PipelineConfig, Source, VerilogLibrary};

#[test]
#[ignore = "requires xlsynth/bedrock-rtl checked out at target/bedrock-rtl (see ci.yml)"]
fn pipeline_interface_matches_bedrock() {
    let sources = [Source::file("target/bedrock-rtl/delay/rtl/br_delay_nr.sv")];
    let library = VerilogLibrary::from_config(&ParserConfig {
        sources: &sources,
        incdirs: &["target/bedrock-rtl/macros"],
        ..Default::default()
    })
    .unwrap();

    let bedrock = library.module("br_delay_nr").unwrap();
    for (width, depth) in [(1, 0), (7, 1), (13, 3)] {
        let model = PipelineConfig {
            depth,
            ..Default::default()
        }
        .to_mod_def(width);
        let rtl = bedrock.parameterize(&[("Width", width), ("NumStages", depth)]);
        let mut ports = rtl.get_ports(None);
        if depth == 0 {
            // The RTL primitive is a wire at depth zero. Bedrock keeps clk for assertions.
            assert!(!model.has_port("clk"));
            ports.retain(|port| port.name() != "clk");
        }
        assert_eq!(model.get_ports(None).len(), ports.len());
        for port in ports {
            let name = port.name();
            let expected = port.io();
            let actual = model.get_port(name).io();
            assert_eq!(actual, expected, "{name} at width {width}, depth {depth}");
        }
    }
}
