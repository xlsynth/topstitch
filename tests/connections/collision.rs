// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn generated_wire_name_avoids_system_verilog_keywords() {
    let leaf = ModDef::new("Leaf");
    leaf.feedthrough("d", "match", 1);
    let top = ModDef::new("Top");
    let first = top.instantiate(&leaf, Some("first"), None);
    first.get_port("d").export_as("data");
    let q = first.get_port("match");
    q.export_as("left");
    q.export_as("right");

    let sv = top.emit(EmitOptions::default());
    assert!(sv.contains(".match(first_match__1)"), "{sv}");
    ModDef::from_verilog("Top", sv).unwrap();
}

#[test]
fn test_inst_name_false_collision() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("i_x", IO::Input(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    let b_inst = top.instantiate(&b, Some("a"), None);

    a_inst.get_port("x").connect(&b_inst.get_port("i_x"));

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top;
  wire [7:0] a_i_x;
  a a_i (
    .x(a_i_x)
  );
  b a (
    .i_x(a_i_x)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Net \"custom\" has already been manually specified")]
fn test_specify_net_name_collision() {
    let a = ModDef::new("a");
    a.add_port("ix", IO::Input(8));
    a.add_port("ox", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("iy", IO::Input(8));
    b.add_port("oy", IO::Output(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    let a_inst = top.instantiate(&a, None, None);
    let b_inst = top.instantiate(&b, None, None);

    a_inst.get_port("ox").connect(&b_inst.get_port("iy"));
    a_inst.get_port("ox").specify_net_name("custom");
    b_inst.get_port("oy").connect(&a_inst.get_port("ix"));
    b_inst.get_port("oy").specify_net_name("custom");

    top.emit(EmitOptions::default());
}

#[test]
fn test_mod_def_name_false_collision() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");
    top.add_port("a_i_x", IO::Input(8)).unused();

    let a_inst = top.instantiate(&a, Some("a_i"), None);
    a_inst.get_port("x").export_as("y");

    assert_eq!(
        top.emit(EmitOptions::default()),
        "\
module Top(
  input wire [7:0] a_i_x,
  output wire [7:0] y
);
  a a_i (
    .x(y)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Net name collision")]
fn test_mod_def_name_collision_with_custom_net_name() {
    let a = ModDef::new("a");
    a.add_port("x", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("y", IO::Input(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");
    top.add_port("custom", IO::Input(8)).unused();

    let a_inst = top.instantiate(&a, None, None);
    let b_inst = top.instantiate(&b, None, None);

    a_inst.get_port("x").connect(&b_inst.get_port("y"));
    a_inst.get_port("x").specify_net_name("custom");

    top.emit(EmitOptions::default());
}

#[test]
#[should_panic(expected = "conflicts with a module instance")]
fn requested_wire_cannot_take_an_instance_name() {
    let pass = ModDef::new("Pass");
    pass.feedthrough("d", "q", 1);
    let top = ModDef::new("Top");
    let stage = top.instantiate(&pass, Some("stage_i"), None);
    stage.get_port("d").export_as("d");
    stage.get_port("q").export_as("q");
    stage.get_port("q").specify_net_name("stage_i");
}

#[test]
fn test_mod_inst_name_collision_with_custom_net_name() {
    let a = ModDef::new("a");
    a.add_port("ix", IO::Input(8));
    a.add_port("ox", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("b");
    b.add_port("iy", IO::Input(8));
    b.add_port("oy", IO::Output(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("Top");

    top.add_port("b_i_oy__1", IO::Input(1)).unused();
    let a_inst = top.instantiate(&a, Some("a_i"), None);
    let b_inst = top.instantiate(&b, Some("b_i"), None);

    a_inst.get_port("ox").connect(&b_inst.get_port("iy"));
    a_inst.get_port("ox").specify_net_name("b_i_oy");
    b_inst.get_port("oy").connect(&a_inst.get_port("ix"));

    let sv = top.emit(EmitOptions::default());
    assert!(sv.contains(".ox(b_i_oy)"));
    assert!(sv.contains(".iy(b_i_oy)"));
    assert!(sv.contains(".oy(b_i_oy__2)"));
    assert!(sv.contains(".ix(b_i_oy__2)"));
}

#[rstest::rstest]
fn test_instance_name_collision_with_port(#[values(false, true)] instance_first: bool) {
    let top = ModDef::new("Top");
    let leaf = ModDef::new("Leaf");
    if instance_first {
        top.instantiate(&leaf, Some("occupied"), None);
        assert_name_collision(|| {
            top.add_port("occupied", IO::Input(1));
        });
        assert!(!top.has_port("occupied"));
    } else {
        top.add_port("occupied", IO::Input(1));
        assert_name_collision(|| {
            top.instantiate(&leaf, Some("occupied"), None);
        });
        assert!(top.get_instances().is_empty());
    }
}

#[rstest::rstest]
fn test_instance_name_collision_with_specified_net(#[values(false, true)] instance_first: bool) {
    let top = ModDef::new("Top");
    let source = ModDef::new("Source");
    source.add_port("data", IO::Output(1));
    let src = top.instantiate(&source, Some("src"), None);
    let leaf = ModDef::new("Leaf");

    if instance_first {
        top.instantiate(&leaf, Some("occupied"), None);
        assert_name_collision(|| src.get_port("data").specify_net_name("occupied"));
        src.get_port("data").specify_net_name("available");
    } else {
        src.get_port("data").specify_net_name("occupied");
        assert_name_collision(|| {
            top.instantiate(&leaf, Some("occupied"), None);
        });
        assert_eq!(top.get_instances().len(), 1);
        top.instantiate(&leaf, Some("available"), None);
    }
}

#[test]
fn generated_wire_avoids_an_existing_instance_name() {
    let source = ModDef::new("Source");
    source.add_port("data", IO::Output(1));
    source.set_usage(Usage::EmitNothingAndStop);
    let sink = ModDef::new("Sink");
    sink.add_port("data", IO::Input(1));
    sink.set_usage(Usage::EmitNothingAndStop);
    let top = ModDef::new("Top");
    let src = top.instantiate(&source, Some("src"), None);
    let dst = top.instantiate(&sink, Some("src_data"), None);
    src.get_port("data").connect(&dst.get_port("data"));
    let sv = top.emit(EmitOptions::default());
    assert!(sv.contains("wire src_data__1;"));
    assert_eq!(sv.matches(".data(src_data__1)").count(), 2);
    let design =
        format!("module Source(output data); endmodule\nmodule Sink(input data); endmodule\n{sv}");
    ModDef::from_verilog("Top", design).unwrap();
}

#[rstest::rstest]
fn test_autoconnect_port_collision_with_instance(#[values(false, true)] existing: bool) {
    let top = ModDef::new("Top");
    let child = ModDef::new("Child");
    child.add_port("safe", IO::Input(1));
    child.add_port("occupied", IO::Input(1));
    let name = if existing {
        top.instantiate(&ModDef::new("Leaf"), Some("occupied"), None);
        "new"
    } else {
        "occupied"
    };

    assert_name_collision(|| {
        top.instantiate(&child, Some(name), Some(&["safe", "occupied"]));
    });
    assert!(!top.has_port("occupied"));
}

#[test]
fn test_stub_ignores_hidden_instance_names() {
    let top = ModDef::new("Top");
    top.add_port("visible", IO::Input(1));
    let source = ModDef::new("Source");
    source.add_port("data", IO::Output(1));
    let sink = ModDef::new("Sink");
    sink.add_port("data", IO::Input(1));
    let src = top.instantiate(&source, Some("src"), None);
    let dst = top.instantiate(&sink, Some("src_data"), None);
    src.get_port("data").connect(&dst.get_port("data"));
    top.set_usage(Usage::EmitStubAndStop);

    let verilog = top.emit(EmitOptions::default());
    assert!(verilog.contains("input wire visible"));
    assert!(!verilog.contains("src_data"));
}

/// Check that a declaration was rejected at the point of construction.
fn assert_name_collision(create: impl FnOnce()) {
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(create))
        .expect_err("expected a declaration name collision");
    let message = error.downcast_ref::<String>().unwrap();
    assert!(
        message.contains("Name collision in module Top"),
        "{message}"
    );
}

#[test]
fn test_instance_and_port_names_in_different_modules() {
    let child = ModDef::new("Child");
    child.add_port("data", IO::Output(1));
    child.set_usage(Usage::EmitStubAndStop);
    let top = ModDef::new("Top");
    top.instantiate(&child, Some("data"), None)
        .get_port("data")
        .export_as("out");

    let verilog = top.emit(EmitOptions::default());
    assert!(verilog.contains("output wire data"));
    assert!(verilog.contains("Child data ("));
}
