// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_stub_recursive() {
    let a_def = ModDef::new("a");
    let b_def = ModDef::new("b");
    let c_def = ModDef::new("skip_c");
    let d_def = ModDef::new("skip_d");
    let e_def = ModDef::new("e");
    let f_def = ModDef::new("f");
    let g_def = ModDef::new("g");

    a_def.instantiate(&b_def, None, None);
    a_def.instantiate(&c_def, None, None);
    b_def.instantiate(&d_def, None, None);
    c_def.instantiate(&e_def, None, None);
    d_def.instantiate(&f_def, None, None);
    e_def.instantiate(&g_def, None, None);

    a_def.stub_recursive("^skip_(.*)$");

    assert_eq!(
        a_def.emit(EmitOptions::default()),
        "\
module skip_d;

endmodule
module b;
  skip_d skip_d_i (

  );
endmodule
module skip_c;

endmodule
module a;
  b b_i (

  );
  skip_c skip_c_i (

  );
endmodule
"
    );
}

#[test]
fn parameterized_stubs_use_concrete_widths_and_preserve_external_overrides() {
    const CELL: &str = "module Bank #(parameter W = 1)
        (input logic [W-1:0] d, output logic [W-1:0] q); assign q = d; endmodule";
    for external in [false, true] {
        let top = ModDef::new("Top");
        let bank = ModDef::from_verilog("Bank", CELL).unwrap();
        for width in [1, 2, 4, 8] {
            let cell = if width == 1 {
                bank.clone()
            } else {
                bank.parameterize(&[("W", width)])
            };
            if !external || width != 8 {
                cell.set_usage(Usage::EmitStubAndStop);
            }
            let inst = top.instantiate(&cell, Some(&format!("bank{width}")), None);
            inst.get_port("d")
                .connect(&top.add_port(format!("d{width}"), IO::Input(width)));
            inst.get_port("q")
                .connect(&top.add_port(format!("q{width}"), IO::Output(width)));
        }
        let sv = top.emit(Default::default());
        assert_eq!(sv.matches(".W(").count(), usize::from(external));
        for width in [1, 2, 4, 8] {
            if external && width == 8 {
                continue;
            }
            let binding = format!(" bank{width} (");
            let stub_name = sv
                .lines()
                .find_map(|line| line.trim().strip_suffix(&binding))
                .expect("missing concrete stub instance");
            let stub = ModDef::from_verilog(stub_name, &sv).unwrap();
            assert_eq!(stub.get_port("d").io(), IO::Input(width));
            assert_eq!(stub.get_port("q").io(), IO::Output(width));
        }
        let source = if external {
            format!("{CELL}\n{sv}")
        } else {
            sv
        };
        ModDef::from_verilog("Top", source).unwrap();
    }
}

#[test]
#[should_panic(expected = "External parameter overrides refer to Bank")]
fn external_parameter_overrides_cannot_bind_to_a_local_module_with_no_parameters() {
    let top = ModDef::new("Top");
    let local = ModDef::new("Bank");
    top.instantiate(&local, Some("local_bank"), None);
    let external =
        ModDef::from_verilog("Bank", "module Bank #(parameter W=1)(); endmodule").unwrap();
    top.instantiate(
        &external.parameterize(&[("W", 2)]),
        Some("external_bank"),
        None,
    );
    top.emit(Default::default());
}
