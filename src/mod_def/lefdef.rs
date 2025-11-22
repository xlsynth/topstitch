// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use indexmap::IndexMap;

use crate::lefdef::{self, DefComponent, DefOrientation, DefPin, DefPoint, LefComponent};
use crate::mod_def::CalculatedPlacement;
use crate::{LefDefOptions, ModDef, Polygon};

impl ModDef {
    /// Emit a LEF string describing this module's geometry and pins.
    pub fn emit_lef(&self, opts: &LefDefOptions) -> String {
        let component = self.to_lef_component(true);
        lefdef::generate_lef(&[component], opts)
    }

    /// Emit a LEF file for this module.
    pub fn emit_lef_to_file<P: AsRef<Path>>(
        &self,
        lef_path: P,
        opts: &LefDefOptions,
    ) -> std::io::Result<()> {
        let lef = self.emit_lef(opts);
        fs::write(lef_path, lef)
    }

    /// Emit a DEF string for this module
    pub fn emit_def(&self, opts: &LefDefOptions) -> String {
        let (placements, mod_defs) = self.collect_placements_and_mod_defs(opts);
        let lef_components: IndexMap<String, LefComponent> = mod_defs
            .iter()
            .map(|(name, md)| (name.clone(), md.to_lef_component(false)))
            .collect();

        lefdef::generate_def(
            &self.get_name(),
            self.get_shape()
                .map(|shape| shape.to_def_die_area())
                .as_ref(),
            &self.to_def_pins(),
            &placements_to_def_components(&placements, &lef_components)
                .into_values()
                .collect::<Vec<_>>(),
            opts,
        )
    }

    /// Emit a DEF file for this module
    pub fn emit_def_to_file<P: AsRef<Path>>(
        &self,
        def_path: P,
        opts: &LefDefOptions,
    ) -> std::io::Result<()> {
        let def = self.emit_def(opts);
        fs::write(def_path, def)
    }

    /// Emit LEF and DEF strings for this module using collected shapes and
    /// placements. Returns (lef_string, def_string).
    pub fn emit_lef_def(&self, opts: &LefDefOptions) -> (String, String) {
        let (placements, mod_defs) = self.collect_placements_and_mod_defs(opts);
        let lef_components: IndexMap<String, LefComponent> = mod_defs
            .iter()
            .map(|(name, md)| (name.clone(), md.to_lef_component(true)))
            .collect();

        let def_components = placements_to_def_components(&placements, &lef_components);
        let lef = lefdef::generate_lef(&lef_components.into_values().collect::<Vec<_>>(), opts);

        let def = lefdef::generate_def(
            &self.get_name(),
            self.get_shape()
                .map(|shape| shape.to_def_die_area())
                .as_ref(),
            &self.to_def_pins(),
            &def_components.into_values().collect::<Vec<_>>(),
            opts,
        );

        (lef, def)
    }

    /// Emit LEF and DEF to files.
    pub fn emit_lef_def_to_files<P1: AsRef<Path>, P2: AsRef<Path>>(
        &self,
        lef_path: P1,
        def_path: P2,
        opts: &LefDefOptions,
    ) -> std::io::Result<()> {
        let (lef, def) = self.emit_lef_def(opts);
        fs::write(lef_path, lef)?;
        fs::write(def_path, def)?;
        Ok(())
    }
}

impl ModDef {
    fn to_lef_component(&self, include_pins: bool) -> LefComponent {
        let core = self.core.borrow();
        let name = core.name.clone();
        let shape = core
            .shape
            .as_ref()
            .unwrap_or_else(|| panic!("Module '{name}' has no shape defined"));
        let bbox = shape.bbox();
        assert!(bbox.min_x >= 0, "LEFs do not support negative coordinates");
        assert!(bbox.min_y >= 0, "LEFs do not support negative coordinates");

        // Construct LEF pins from physical_pins in a deterministic order
        let mut lef_pins = Vec::new();
        if include_pins {
            for (port_name, pins) in core.physical_pins.iter() {
                let port = core.ports.get(port_name).unwrap_or_else(|| {
                    panic!(
                        "Physical pin defined for unknown port {}.{port_name}",
                        core.name
                    )
                });

                for (bit, maybe_pin) in pins.iter().enumerate() {
                    if let Some(pin) = maybe_pin {
                        let name = if port.width() > 1 {
                            format!("{}[{}]", &port_name, bit)
                        } else {
                            port_name.clone()
                        };
                        let polygon_abs: Vec<(i64, i64)> = pin
                            .transformed_polygon()
                            .0
                            .iter()
                            .map(|c| (c.x, c.y))
                            .collect();
                        lef_pins.push(crate::lefdef::LefPin {
                            name,
                            direction: port.to_lef_direction(),
                            shape: crate::lefdef::LefShape {
                                layer: pin.layer.clone(),
                                polygon: polygon_abs,
                            },
                        });
                    }
                }
            }
        }

        let layer_name = core
            .layer
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "OUTLINE".to_string());

        LefComponent {
            name,
            width: bbox.max_x,
            height: bbox.max_y,
            shape: crate::lefdef::LefShape {
                layer: layer_name,
                polygon: shape.0.iter().map(|p| (p.x, p.y)).collect(),
            },
            pins: lef_pins,
        }
    }

    fn to_def_pins(&self) -> Vec<DefPin> {
        let core = self.core.borrow();

        // Construct DEF pins from physical_pins in a deterministic order
        let mut def_pins = Vec::new();
        for (port_name, pins) in core.physical_pins.iter() {
            let port = core.ports.get(port_name).unwrap_or_else(|| {
                panic!(
                    "Physical pin defined for unknown port {}.{port_name}",
                    core.name
                )
            });

            for (bit, maybe_pin) in pins.iter().enumerate() {
                if let Some(pin) = maybe_pin {
                    let name = if port.width() > 1 {
                        format!("{}[{}]", &port_name, bit)
                    } else {
                        port_name.clone()
                    };
                    let bbox = pin.polygon.bbox();
                    let position = pin.translation();
                    def_pins.push(DefPin {
                        name: name.clone(),
                        direction: port.to_def_direction(),
                        // TODO(sherbst) 2025-11-17: support other pin uses?
                        pin_use: "SIGNAL".to_string(),
                        layer: pin.layer.clone(),
                        // TODO(sherbst) 2025-11-17: support non-rectangular pins?
                        shape: (
                            DefPoint {
                                x: bbox.min_x,
                                y: bbox.min_y,
                            },
                            DefPoint {
                                x: bbox.max_x,
                                y: bbox.max_y,
                            },
                        ),
                        position: DefPoint {
                            x: position.x,
                            y: position.y,
                        },
                        orientation: DefOrientation::from_orientation(
                            pin.transform.as_orientation(),
                        ),
                    });
                }
            }
        }

        def_pins
    }
}

fn placements_to_def_components(
    placements: &IndexMap<String, CalculatedPlacement>,
    lef_components: &IndexMap<String, LefComponent>,
) -> IndexMap<String, DefComponent> {
    placements
        .iter()
        .map(|(inst_name, p)| {
            let lef_component = lef_components.get(&p.module).unwrap_or_else(|| {
                let module = &p.module;
                panic!("No LEF component found for module '{module}' (needed for DEF placement)")
            });
            let bbox = Polygon::from_width_height(lef_component.width, lef_component.height)
                .apply_transform(&p.transform)
                .bbox();
            (
                inst_name.clone(),
                DefComponent {
                    inst_name: inst_name.clone(),
                    macro_name: p.module.clone(),
                    // Note that DEF placement is for the lower-left corner of the shape
                    x: bbox.min_x,
                    y: bbox.min_y,
                    orientation: lefdef::DefOrientation::from_orientation(
                        p.transform.as_orientation(),
                    ),
                },
            )
        })
        .collect()
}
