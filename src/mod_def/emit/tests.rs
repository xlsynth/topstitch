// SPDX-License-Identifier: Apache-2.0

use super::{EmissionHierarchy, EmitOptions};
use crate::{GateMap, IO, ModDef, Primitive, PrimitivePolicy, Usage};

#[test]
fn a_late_child_of_a_shared_definition_waits_for_the_next_emit() {
    let top = ModDef::new("Top");
    let shared = ModDef::new("Shared");
    top.instantiate(&shared, Some("first"), None);
    top.instantiate(&shared, Some("second"), None);
    let plan = EmissionHierarchy::new(&top);

    shared.instantiate(&ModDef::new("Late"), Some("late"), None);

    let old = plan.emit(&EmitOptions::default());
    let fresh = top.emit(EmitOptions::default());
    assert!(!old.contains("Late"));
    assert_eq!(fresh.matches("module Late;").count(), 1);
    assert_eq!(fresh.matches("Late late (").count(), 1);
    for sv in [old, fresh] {
        assert_eq!(sv.matches("module Shared;").count(), 1);
        assert!(sv.contains("Shared first (") && sv.contains("Shared second ("));
        ModDef::from_verilog("Top", sv).unwrap();
    }
}

#[test]
fn opening_a_captured_stub_reveals_its_child_only_in_the_next_emit() {
    let top = ModDef::new("Top");
    let partition = ModDef::new("Partition");
    partition.instantiate(&ModDef::new("Leaf"), Some("leaf"), None);
    top.instantiate(&partition, Some("partition"), None);
    partition.set_usage(Usage::EmitStubAndStop);
    let plan = EmissionHierarchy::new(&top);

    partition.set_usage(Usage::EmitDefinitionAndDescend);

    let old = plan.emit(&EmitOptions::default());
    assert!(old.contains("module Partition;") && old.contains("Partition partition ("));
    assert!(!old.contains("Leaf"));
    let fresh = top.emit(EmitOptions::default());
    assert!(fresh.contains("module Leaf;") && fresh.contains("Leaf leaf ("));
    for sv in [old, fresh] {
        ModDef::from_verilog("Top", sv).unwrap();
    }
}

#[test]
fn a_late_module_name_collision_renames_the_helper_only_in_the_next_emit() {
    let cell_sv = "module Inv(input logic a, output logic y); assign y = ~a; endmodule";
    let mut map = GateMap::new();
    map.inv(ModDef::from_verilog("Inv", cell_sv).unwrap(), "a", "y")
        .unwrap();
    let top = ModDef::new("Top");
    top.set_primitive_policy(PrimitivePolicy::RequireMapping(map))
        .unwrap();
    let invert = top.instantiate(&top.primitive_def(Primitive::Not, 1), Some("invert"), None);
    invert
        .get_port("a")
        .connect(&top.add_port("a", IO::Input(1)));
    top.add_port("y", IO::Output(1))
        .connect(&invert.get_port("y"));
    let preferred = invert.get_mod_def().get_name();
    let plan = EmissionHierarchy::new(&top);

    top.instantiate(&ModDef::new(&preferred), Some("ordinary"), None);

    let old = plan.emit(&EmitOptions::default());
    assert!(old.contains(&format!("module {preferred}(")));
    assert!(old.contains(&format!("{preferred} invert (")));
    assert!(!old.contains(" ordinary ("));

    let fresh = top.emit(EmitOptions::default());
    assert!(fresh.contains(&format!("module {preferred};")));
    assert!(fresh.contains(&format!("{preferred} ordinary (")));
    assert!(fresh.contains(&format!("module {preferred}_1(")));
    assert!(fresh.contains(&format!("{preferred}_1 invert (")));
    for sv in [old, fresh] {
        assert_eq!(sv.matches("Inv bit_0 (").count(), 1);
        ModDef::from_verilog("Top", format!("{cell_sv}\n{sv}")).unwrap();
    }
}
