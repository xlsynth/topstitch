// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use indexmap::IndexMap;

use crate::lefdef::{self, DefComponent, DefOrientation, DefPin, DefPoint, LefComponent};
use crate::mod_def::CalculatedPlacement;
use crate::mod_def::lef_parse::mod_defs_from_lef;
use crate::validate::pins_contained;
use crate::{LefDefOptions, ModDef, Polygon};

impl ModDef {
    /// Create a ModDef from a LEF string. Panics if the LEF contains zero or
    /// multiple macros.
    pub fn from_lef(lef: impl AsRef<str>, opts: &LefDefOptions) -> Self {
        let mut mods = Self::all_from_lef(lef, opts);
        match mods.len() {
            0 => panic!("No LEF macros found."),
            1 => mods.remove(0),
            _ => panic!("Multiple LEF macros found. Use all_from_lef instead."),
        }
    }

    /// Create ModDefs from all macros in a LEF string.
    pub fn all_from_lef(lef: impl AsRef<str>, opts: &LefDefOptions) -> Vec<Self> {
        mod_defs_from_lef(lef.as_ref(), opts)
    }

    /// Create a ModDef from a LEF file. Panics if the LEF contains zero or
    /// multiple macros.
    pub fn from_lef_file<P: AsRef<Path>>(lef_path: P, opts: &LefDefOptions) -> Self {
        let lef = fs::read_to_string(lef_path).expect("Failed to read LEF file");
        Self::from_lef(lef, opts)
    }

    /// Create ModDefs from all macros in a LEF file.
    pub fn all_from_lef_file<P: AsRef<Path>>(lef_path: P, opts: &LefDefOptions) -> Vec<Self> {
        let lef = fs::read_to_string(lef_path).expect("Failed to read LEF file");
        Self::all_from_lef(lef, opts)
    }

    /// Emit a LEF string describing this module's geometry and pins.
    pub fn emit_lef(&self, opts: &LefDefOptions) -> String {
        let component = self.to_lef_component(opts);
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
            .map(|(name, md)| {
                (
                    name.clone(),
                    md.to_lef_component(&LefDefOptions {
                        // component pins are not included in this case because we only need to generate
                        // LefComponents to get placement information.
                        include_pins: false,
                        ..opts.clone()
                    }),
                )
            })
            .collect();

        lefdef::generate_def(
            &self.get_name(),
            self.get_shape()
                .map(|shape| shape.to_def_die_area())
                .as_ref(),
            &self.to_def_pins(opts),
            &placements_to_def_components(&placements, &lef_components, opts)
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
            .map(|(name, md)| (name.clone(), md.to_lef_component(opts)))
            .collect();

        let def_components = placements_to_def_components(&placements, &lef_components, opts);
        let lef = lefdef::generate_lef(&lef_components.into_values().collect::<Vec<_>>(), opts);

        let def = lefdef::generate_def(
            &self.get_name(),
            self.get_shape()
                .map(|shape| shape.to_def_die_area())
                .as_ref(),
            &self.to_def_pins(opts),
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
    fn to_lef_component(&self, opts: &LefDefOptions) -> LefComponent {
        let core = self.core.borrow();
        let name = core.name.clone();
        let shape = core
            .shape
            .as_ref()
            .unwrap_or_else(|| panic!("Module '{name}' has no shape defined"));
        let bbox = shape.bbox();
        assert!(bbox.min_x >= 0, "LEFs do not support negative coordinates");
        assert!(bbox.min_y >= 0, "LEFs do not support negative coordinates");

        let (open_char, close_char) = opts.open_close_chars();

        // Construct LEF pins from physical_pins in a deterministic order. Also keep track of pins
        // for checking that they are contained within the ModDef shape if that option is enabled.
        let mut lef_pins = Vec::new();
        if opts.include_pins {
            let mut pins_for_check = Vec::new();

            for (port_name, pins) in core.physical_pins.iter() {
                let port = core.ports.get(port_name).unwrap_or_else(|| {
                    panic!(
                        "Physical pin defined for unknown port {}.{port_name}",
                        core.name
                    )
                });

                let port_width = port.width();

                for (bit, maybe_pin) in pins.iter().enumerate() {
                    if let Some(pin) = maybe_pin {
                        let name =
                            lef_def_pin_name(port_name, port_width, bit, open_char, close_char);
                        let transformed_polygon = pin.transformed_polygon();

                        if opts.check_that_pins_are_contained {
                            pins_for_check.push((name.clone(), transformed_polygon.clone()));
                        }

                        let polygon_abs: Vec<(i64, i64)> =
                            transformed_polygon.0.iter().map(|c| (c.x, c.y)).collect();
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

            // Check that pins are contained within the ModDef shape if that option is enabled.
            if opts.check_that_pins_are_contained {
                pins_contained::check(&self.get_name(), shape, &pins_for_check);
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

    fn to_def_pins(&self, opts: &LefDefOptions) -> Vec<DefPin> {
        let core = self.core.borrow();

        let (open_char, close_char) = opts.open_close_chars();

        // Construct DEF pins from physical_pins in a deterministic order. Also keep track of pins
        // for checking that they are contained within the ModDef shape if that option is enabled.
        let mut def_pins = Vec::new();
        let mut pins_for_check = Vec::new();
        for (port_name, pins) in core.physical_pins.iter() {
            let port = core.ports.get(port_name).unwrap_or_else(|| {
                panic!(
                    "Physical pin defined for unknown port {}.{port_name}",
                    core.name
                )
            });

            let port_width = port.width();

            for (bit, maybe_pin) in pins.iter().enumerate() {
                if let Some(pin) = maybe_pin {
                    let name = lef_def_pin_name(port_name, port_width, bit, open_char, close_char);
                    if opts.check_that_pins_are_contained {
                        pins_for_check.push((name.clone(), pin.transformed_polygon()));
                    }
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

        // Check that pins are contained within the ModDef shape if that option is enabled.
        if opts.check_that_pins_are_contained
            && let Some(shape) = self.get_shape()
        {
            pins_contained::check(&self.get_name(), &shape, &pins_for_check);
        }

        def_pins
    }
}

fn placements_to_def_components(
    placements: &IndexMap<String, CalculatedPlacement>,
    lef_components: &IndexMap<String, LefComponent>,
    opts: &LefDefOptions,
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
            if (!opts.macros_exempt_from_grid_check.contains(&p.module))
                && (!opts.instances_exempt_from_grid_check.contains(inst_name))
            {
                if let Some((x_grid, y_grid)) = opts.check_grid_placement {
                    if (bbox.min_x % x_grid) != 0 {
                        panic!(
                            "Instance {} of macro {} is not placed on the X grid",
                            inst_name, p.module
                        );
                    } else if (bbox.min_y % y_grid) != 0 {
                        panic!(
                            "Instance {} of macro {} is not placed on the Y grid",
                            inst_name, p.module
                        );
                    }
                }
                if let Some((x_grid, y_grid)) = opts.check_grid_size {
                    if (bbox.get_width() % x_grid) != 0 {
                        panic!(
                            "Instance {} of macro {} is not sized to a multiple of the X grid",
                            inst_name, p.module
                        );
                    } else if (bbox.get_height() % y_grid) != 0 {
                        panic!(
                            "Instance {} of macro {} is not sized to a multiple of the Y grid",
                            inst_name, p.module
                        );
                    }
                }
            }
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

fn lef_def_pin_name(
    port_name: &str,
    port_width: usize,
    bit: usize,
    open_char: char,
    close_char: char,
) -> String {
    if port_width == 1 {
        port_name.to_string()
    } else {
        format!("{}{}{}{}", &port_name, open_char, bit, close_char)
    }
}
