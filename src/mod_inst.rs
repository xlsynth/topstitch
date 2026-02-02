// SPDX-License-Identifier: Apache-2.0

use std::hash::{Hash, Hasher};

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use num_bigint::BigInt;

use crate::{
    BoundingBox, ConvertibleToModDef, Intf, MetadataKey, MetadataValue, ModDef, ModDefCore, Port,
    PortSlice,
};
use crate::{Coordinate, Mat3, Orientation, PhysicalPin, Placement};

/// Represents an instance of a module definition, like `<mod_def_name>
/// <mod_inst_name> ( ... );` in Verilog.
#[derive(Clone, Debug)]
pub struct HierPathElem {
    pub(crate) mod_def_core: Weak<RefCell<ModDefCore>>,
    pub(crate) inst_name: String,
}

impl PartialEq for HierPathElem {
    fn eq(&self, other: &Self) -> bool {
        match (self.mod_def_core.upgrade(), other.mod_def_core.upgrade()) {
            (Some(a_rc), Some(b_rc)) => {
                Rc::ptr_eq(&a_rc, &b_rc) && (self.inst_name == other.inst_name)
            }
            _ => false,
        }
    }
}

impl Hash for HierPathElem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mod_def_core
            .upgrade()
            .unwrap()
            .borrow()
            .name
            .hash(state);
        self.inst_name.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct ModInst {
    pub(crate) hierarchy: Vec<HierPathElem>,
}

impl ModInst {
    pub(crate) fn mod_def_core_where_instantiated(&self) -> Rc<RefCell<ModDefCore>> {
        self.hierarchy
            .last()
            .expect("ModInst hierarchy cannot be empty")
            .mod_def_core
            .upgrade()
            .expect("Containing ModDefCore has been dropped")
    }

    pub(crate) fn mod_def_core_of_instance(&self) -> Rc<RefCell<ModDefCore>> {
        let inst_name = self.name().to_string();
        self.mod_def_core_where_instantiated()
            .borrow()
            .instances
            .get(&inst_name)
            .unwrap_or_else(|| panic!("Instance named {} not found", inst_name))
            .clone()
    }

    /// Returns the name of this module instance.
    pub fn name(&self) -> &str {
        &self
            .hierarchy
            .last()
            .expect("ModInst hierarchy cannot be empty")
            .inst_name
    }

    pub fn set_metadata(
        &self,
        key: impl Into<MetadataKey>,
        value: impl Into<MetadataValue>,
    ) -> Self {
        let inst_name = self.name().to_string();
        let core_rc = self.mod_def_core_where_instantiated();
        let mut core = core_rc.borrow_mut();
        core.mod_inst_metadata
            .entry(inst_name)
            .or_default()
            .insert(key.into(), value.into());
        self.clone()
    }

    pub fn get_metadata(&self, key: impl AsRef<str>) -> Option<MetadataValue> {
        let inst_name = self.name().to_string();
        let core_rc = self.mod_def_core_where_instantiated();
        let core = core_rc.borrow();
        core.mod_inst_metadata
            .get(&inst_name)
            .and_then(|metadata| metadata.get(key.as_ref()).cloned())
    }

    pub fn clear_metadata(&self, key: impl AsRef<str>) -> Self {
        let inst_name = self.name().to_string();
        let core_rc = self.mod_def_core_where_instantiated();
        let mut core = core_rc.borrow_mut();
        if let Some(metadata) = core.mod_inst_metadata.get_mut(&inst_name) {
            metadata.remove(key.as_ref());
            if metadata.is_empty() {
                core.mod_inst_metadata.remove(&inst_name);
            }
        }
        self.clone()
    }

    /// Returns `true` if this module instance has an interface with the given
    /// name.
    pub fn has_intf(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core_of_instance(),
        }
        .has_intf(name)
    }

    /// Returns `true` if this module instance has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        ModDef {
            core: self.mod_def_core_of_instance(),
        }
        .has_port(name)
    }

    /// First, get the module definition for this instance. Then, return the
    /// module instance with the given name in that module defintion.
    pub fn get_instance(&self, name: impl AsRef<str>) -> ModInst {
        let child = self.get_mod_def().get_instance(name.as_ref());
        let mut combined = self.hierarchy.clone();
        for frame in &child.hierarchy {
            combined.push(frame.clone());
        }
        ModInst {
            hierarchy: combined,
        }
    }

    /// Returns a vector of all module instances within this module instance.
    pub fn get_instances(&self) -> Vec<ModInst> {
        self.get_mod_def()
            .get_instances()
            .into_iter()
            .map(|child| {
                let mut combined = self.hierarchy.clone();
                combined.extend(child.hierarchy);
                ModInst {
                    hierarchy: combined,
                }
            })
            .collect()
    }

    /// Marks all ports on this instance as unused or ties them off to the given
    /// value.
    pub fn unused_and_tieoff<T: Into<BigInt> + Clone>(&self, value: T) {
        let value_as_big_int = value.into();

        for port in self.get_ports(None) {
            port.unused_or_tieoff(value_as_big_int.clone());
        }
    }

    /// Returns the cumulative placement transform for this instance, combining
    /// every placed level in the hierarchy.
    pub fn get_transform(&self) -> Mat3 {
        let mut total = Mat3::identity();

        for frame in &self.hierarchy {
            let core = frame.mod_def_core.upgrade().unwrap_or_else(|| {
                panic!(
                    "Containing ModDefCore for '{}' has been dropped",
                    frame.inst_name
                )
            });

            let placement = {
                let core_borrowed = core.borrow();
                core_borrowed.inst_placements.get(&frame.inst_name).copied()
            };

            if let Some(placement) = placement {
                total = &total * &placement.transform();
            }
        }

        total
    }

    /// Returns the port on this instance with the given name. Panics if no such
    /// port exists.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        ModDef {
            core: self.mod_def_core_of_instance(),
        }
        .get_port(name)
        .assign_to_inst(self)
    }

    /// Returns a slice of the port on this instance with the given name, from
    /// `msb` down to `lsb`, inclusive. Panics if no such port exists.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of ports on this instance with the given prefix, or all
    /// ports if `prefix` is `None`.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let result = ModDef {
            core: self.mod_def_core_of_instance(),
        }
        .get_ports(prefix);
        result
            .into_iter()
            .map(|port| port.assign_to_inst(self))
            .collect()
    }

    /// Validates connection distances for this instance only. This does not
    /// descend into child instances.
    pub fn validate_connection_distances(&self) {
        let self_mod_def = self.get_mod_def();
        let self_mod_def_core_borrowed = self_mod_def.core.borrow();

        let self_transform = self.get_transform();

        for (self_port_name, self_io) in self_mod_def_core_borrowed.ports.iter() {
            let self_port = self.get_port(self_port_name);
            let self_width = self_io.width();

            if self_width == 0 {
                continue;
            }

            let physical_pins =
                if let Some(pins) = self_mod_def_core_borrowed.physical_pins.get(self_port_name) {
                    pins
                } else {
                    continue;
                };

            let max_distances = self_mod_def_core_borrowed
                .port_max_distances
                .get(self_port_name);

            if max_distances.is_none()
                && self_mod_def_core_borrowed
                    .default_connection_max_distance
                    .is_none()
            {
                continue;
            }

            for bit in 0..self_width {
                let self_physical_pin = if let Some(physical_pin) =
                    physical_pins.get(bit).and_then(|pin| pin.as_ref())
                {
                    physical_pin
                } else {
                    continue;
                };

                let max_distance = if let Some(max_distances) = max_distances {
                    if let Some(max_distance) = max_distances.get(bit) {
                        *max_distance
                    } else {
                        self_mod_def_core_borrowed.default_connection_max_distance
                    }
                } else {
                    self_mod_def_core_borrowed.default_connection_max_distance
                };

                let max_distance = if let Some(max_distance) = max_distance {
                    max_distance
                } else {
                    continue;
                };

                let self_port_slice = self_port.bit(bit);

                let other_port_slice = match self_port_slice.trace_through_hierarchy() {
                    Some(other) => other,
                    None => continue,
                };

                if other_port_slice.lsb != other_port_slice.msb {
                    panic!(
                        "Found multi-bit port slice {} when validating connection distance for bit {}",
                        other_port_slice.debug_string(),
                        self.debug_string()
                    );
                }

                let other_mod_inst =
                    if let Some(other_mod_inst) = other_port_slice.port.get_mod_inst() {
                        other_mod_inst
                    } else {
                        // Top-level port
                        continue;
                    };

                let other_transform = other_mod_inst.get_transform();
                let other_port_name = other_port_slice.port.name();
                let other_mod_def_core = other_mod_inst.get_mod_def().core;
                let other_mod_def_core_borrowed = other_mod_def_core.borrow();
                let other_physical_pins = if let Some(other_physical_pins) =
                    other_mod_def_core_borrowed
                        .physical_pins
                        .get(other_port_name)
                {
                    other_physical_pins
                } else {
                    panic!(
                        "Found no physical pins for {} when validating connected bit {}",
                        other_port_slice.debug_string(),
                        self_port_slice.debug_string()
                    );
                };

                let other_physical_pin = if let Some(other_physical_pin) = other_physical_pins
                    .get(other_port_slice.lsb)
                    .and_then(|pin| pin.as_ref())
                {
                    other_physical_pin
                } else {
                    panic!(
                        "Found no physical pins for {} when validating connected bit {}",
                        other_port_slice.debug_string(),
                        self_port_slice.debug_string()
                    );
                };

                let self_pin_bbox = self_physical_pin
                    .transformed_polygon()
                    .apply_transform(&self_transform)
                    .bbox();
                let other_pin_bbox: BoundingBox = other_physical_pin
                    .transformed_polygon()
                    .apply_transform(&other_transform)
                    .bbox();

                let manhattan_distance = self_pin_bbox.gap(&other_pin_bbox);

                assert!(
                    manhattan_distance <= max_distance,
                    "Distance between {} and {} is {}, exceeding the max specified distance of {}",
                    self_port_slice.debug_string(),
                    other_port_slice.debug_string(),
                    manhattan_distance,
                    max_distance,
                );
            }
        }
    }

    /// Returns the interface on this instance with the given name. Panics if no
    /// such interface exists.
    pub fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        let mod_def_core = self.mod_def_core_where_instantiated();
        let instances = &mod_def_core.borrow().instances;

        let inst_core = match instances.get(self.name()) {
            Some(inst_core) => inst_core.clone(),
            None => panic!(
                "Interface '{}' does not exist on module definition '{}'",
                name.as_ref(),
                mod_def_core.borrow().name
            ),
        };

        let inst_core_borrowed = inst_core.borrow();

        if inst_core_borrowed.interfaces.contains_key(name.as_ref()) {
            Intf::ModInst {
                intf_name: name.as_ref().to_string(),
                hierarchy: self.hierarchy.clone(),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in instance '{}'",
                name.as_ref(),
                self.debug_string()
            );
        }
    }

    /// Returns a vector of all interfaces on this module instance with the
    /// given prefix. If `prefix` is `None`, returns all interfaces.
    pub fn get_intfs(&self, prefix: Option<&str>) -> Vec<Intf> {
        self.get_mod_def()
            .get_intfs(prefix)
            .into_iter()
            .map(|intf| match intf {
                Intf::ModDef { name, .. } => Intf::ModInst {
                    intf_name: name,
                    hierarchy: self.hierarchy.clone(),
                },
                Intf::ModInst { .. } => intf,
            })
            .collect()
    }

    /// Returns the ModDef that this is an instance of.
    pub fn get_mod_def(&self) -> ModDef {
        ModDef {
            core: self.mod_def_core_of_instance(),
        }
    }

    pub(crate) fn debug_string(&self) -> String {
        let mut parts = Vec::new();
        if let Some(frame) = self.hierarchy.first() {
            parts.push(frame.mod_def_core.upgrade().unwrap().borrow().name.clone());
        }
        for frame in &self.hierarchy {
            parts.push(frame.inst_name.clone());
        }
        parts.join(".")
    }

    /// Define a physical pin for this instance. The provided `pin` transform is
    /// interpreted in the parent module's coordinate space.
    pub fn place_pin(&self, port_name: impl AsRef<str>, bit: usize, pin: PhysicalPin) {
        let inverse = self.get_transform().inverse();
        let local_transform = &inverse * &pin.transform;
        let local_pin =
            PhysicalPin::from_transform(pin.layer.clone(), pin.polygon.clone(), local_transform);

        self.get_mod_def().place_pin(port_name, bit, local_pin);
    }

    /// Place this instance at a coordinate with an orientation.
    pub fn place<C: Into<Coordinate>>(&self, coordinate: C, orientation: Orientation) {
        let core = self.mod_def_core_where_instantiated();
        core.borrow_mut().inst_placements.insert(
            self.name().to_string(),
            Placement {
                coordinate: coordinate.into(),
                orientation,
            },
        );
    }
}

impl ConvertibleToModDef for ModInst {
    fn to_mod_def(&self) -> ModDef {
        self.get_mod_def()
    }
    fn get_port(&self, name: impl AsRef<str>) -> Port {
        self.get_port(name)
    }
    fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        self.get_intf(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IO, Polygon};

    #[test]
    fn mod_inst_hierarchy_extends_with_get_instance() {
        let mod_c = ModDef::new("C");

        let mod_b = ModDef::new("B");
        mod_b.instantiate(&mod_c, Some("c_inst"), None);

        let mod_a = ModDef::new("A");
        mod_a.instantiate(&mod_b, Some("b_inst"), None);

        let b_inst = mod_a.get_instance("b_inst");
        assert_eq!(b_inst.debug_string(), "A.b_inst");
        assert_eq!(b_inst.hierarchy.len(), 1);
        assert_eq!(b_inst.hierarchy[0].inst_name, "b_inst");
        assert_eq!(
            b_inst.hierarchy[0]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "A"
        );

        let c_from_b = b_inst.get_instance("c_inst");
        assert_eq!(c_from_b.debug_string(), "A.b_inst.c_inst");
        assert_eq!(c_from_b.name(), "c_inst");
        assert_eq!(c_from_b.get_mod_def().get_name(), "C");
        assert_eq!(c_from_b.hierarchy.len(), 2);
        assert_eq!(c_from_b.hierarchy[0].inst_name, "b_inst");
        assert_eq!(c_from_b.hierarchy[1].inst_name, "c_inst");
        assert_eq!(
            c_from_b.hierarchy[0]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "A"
        );
        assert_eq!(
            c_from_b.hierarchy[1]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "B"
        );

        let c_direct = mod_b.get_instance("c_inst");
        assert_eq!(c_direct.debug_string(), "B.c_inst");
        assert_eq!(c_direct.hierarchy.len(), 1);
        assert_eq!(c_direct.hierarchy[0].inst_name, "c_inst");
        assert_eq!(
            c_direct.hierarchy[0]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "B"
        );
    }

    #[test]
    fn mod_inst_hierarchy_extends_with_get_instances() {
        let leaf = ModDef::new("Leaf");

        let mid = leaf.wrap(Some("Mid"), Some("leaf_inst"));
        let top = mid.wrap(Some("Top"), Some("mid_inst"));

        let mid_inst = top.get_instance("mid_inst");
        let children = mid_inst.get_instances();
        assert_eq!(children.len(), 1);

        let child = &children[0];
        assert_eq!(child.debug_string(), "Top.mid_inst.leaf_inst");
        assert_eq!(child.name(), "leaf_inst");
        assert_eq!(child.get_mod_def().get_name(), "Leaf");
        assert_eq!(child.hierarchy.len(), 2);
        assert_eq!(child.hierarchy[0].inst_name, "mid_inst");
        assert_eq!(child.hierarchy[1].inst_name, "leaf_inst");
        assert_eq!(
            child.hierarchy[0]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "Top"
        );
        assert_eq!(
            child.hierarchy[1]
                .mod_def_core
                .upgrade()
                .unwrap()
                .borrow()
                .name,
            "Mid"
        );
    }

    #[test]
    fn mod_inst_debug_string_handles_deep_hierarchy() {
        let mod_d = ModDef::new("D");

        let mod_c = ModDef::new("C");
        mod_c.instantiate(&mod_d, Some("d_inst"), None);

        let mod_b = ModDef::new("B");
        mod_b.instantiate(&mod_c, Some("c_inst"), None);

        let mod_a = ModDef::new("A");
        mod_a.instantiate(&mod_b, Some("b_inst"), None);

        let d_from_a = mod_a
            .get_instance("b_inst")
            .get_instance("c_inst")
            .get_instance("d_inst");

        assert_eq!(d_from_a.debug_string(), "A.b_inst.c_inst.d_inst");
        assert_eq!(d_from_a.hierarchy.len(), 3);
        assert_eq!(d_from_a.hierarchy[0].inst_name, "b_inst");
        assert_eq!(d_from_a.hierarchy[1].inst_name, "c_inst");
        assert_eq!(d_from_a.hierarchy[2].inst_name, "d_inst");
    }

    #[test]
    fn mod_inst_transform_and_port_coordinate() {
        let leaf = ModDef::new("Leaf");
        leaf.add_port("p", IO::Output(1));
        let pin = PhysicalPin::from_translation(
            "M1",
            Polygon::from_width_height(1, 1),
            Coordinate { x: 2, y: 3 },
        );
        leaf.place_pin("p", 0, pin);

        let mid = ModDef::new("Mid");
        let leaf_inst = mid.instantiate(&leaf, Some("leaf"), None);
        leaf_inst.place((5, 0), Orientation::R90);

        let top = ModDef::new("Top");
        let mid_inst = top.instantiate(&mid, Some("mid"), None);
        mid_inst.place((10, -2), Orientation::R0);

        let leaf_from_top = mid_inst.get_instance("leaf");
        let total_transform = leaf_from_top.get_transform();

        let pin_world = leaf_from_top.get_port("p").bit(0).get_coordinate();
        assert_eq!(
            pin_world,
            Coordinate { x: 2, y: 3 }.apply_transform(&total_transform)
        );
        assert_eq!(pin_world, Coordinate { x: 12, y: 0 });

        let recovered_local = pin_world.apply_transform(&total_transform.inverse());
        assert_eq!(recovered_local, Coordinate { x: 2, y: 3 });

        // Re-place the pin through the instance using parent-space coordinates.
        let polygon = Polygon::from_width_height(1, 1);
        let world_pin = PhysicalPin::from_translation("M1", polygon.clone(), pin_world);
        leaf_from_top.place_pin("p", 0, world_pin);

        // The underlying module stores pins in local coordinates.
        let local_coord = leaf.get_port("p").bit(0).get_coordinate();
        assert_eq!(local_coord, Coordinate { x: 2, y: 3 });
    }

    #[test]
    fn mod_inst_place_pin_inverts_transform() {
        let child = ModDef::new("Child");
        child.add_port("x", IO::Output(1));

        let parent = ModDef::new("Parent");
        let child_inst = parent.instantiate(&child, Some("c"), None);
        child_inst.place((10, 5), Orientation::R180);

        let world_coord = Coordinate { x: 8, y: 7 };
        let polygon = Polygon::from_width_height(2, 3);
        let world_pin = PhysicalPin::from_translation("M2", polygon.clone(), world_coord);
        child_inst.place_pin("x", 0, world_pin);

        // The stored pin should reside in child-local space.
        let core = child.core.borrow();
        let pins = core.physical_pins.get("x").unwrap();
        let stored_pin = pins[0].as_ref().unwrap();
        let expected_local = world_coord.apply_transform(&child_inst.get_transform().inverse());
        assert_eq!(stored_pin.translation(), expected_local);
    }
}
