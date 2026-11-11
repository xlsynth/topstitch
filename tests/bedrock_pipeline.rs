// SPDX-License-Identifier: Apache-2.0

use std::mem::discriminant;
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

    for (width, depth) in [(1, 0), (7, 1), (13, 3)] {
        let model = PipelineConfig {
            depth,
            ..Default::default()
        }
        .to_mod_def(width);
        let rtl = library
            .module(&model.get_name())
            .unwrap()
            .parameterize(&[("Width", width), ("NumStages", depth)]);
        let ports = rtl.get_ports(None);
        assert_eq!(model.get_ports(None).len(), ports.len());
        for port in ports {
            let name = port.name();
            let expected = port.io();
            let actual = model.get_port(name).io();
            assert_eq!(
                discriminant(&actual),
                discriminant(&expected),
                "{name} direction"
            );
            assert_eq!(
                actual.width(),
                expected.width(),
                "{name} width at depth {depth}"
            );
        }
    }
}
