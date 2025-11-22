// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use crate::Orientation;

/// Options that affect both LEF and DEF generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LefDefOptions {
    /// Hierarchy separator. Default: "/".
    pub divider_char: String,
    /// Bus bit characters. Default: "[]".
    pub bus_bit_chars: String,
    /// Micron database units. Default: 1.
    pub units_microns: i64,
    /// If true, the hierarchical path omits the top-level module name.
    pub omit_top_module_in_hierarchy: bool,
    /// If true, include pins in LEF/DEF output.
    pub include_pins: bool,
    /// If true, include obstructions in LEF output.
    pub include_obstructions: bool,
    /// If true, include labels in LEF output.
    pub include_labels: bool,
}

impl Default for LefDefOptions {
    fn default() -> Self {
        LefDefOptions {
            divider_char: "/".to_string(),
            bus_bit_chars: "[]".to_string(),
            units_microns: 1,
            omit_top_module_in_hierarchy: true,
            include_pins: true,
            include_obstructions: true,
            include_labels: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefOrientation {
    N,
    S,
    E,
    W,
    FN,
    FS,
    FE,
    FW,
}

impl DefOrientation {
    pub fn as_str(&self) -> &'static str {
        match self {
            DefOrientation::N => "N",
            DefOrientation::S => "S",
            DefOrientation::E => "E",
            DefOrientation::W => "W",
            DefOrientation::FN => "FN",
            DefOrientation::FS => "FS",
            DefOrientation::FE => "FE",
            DefOrientation::FW => "FW",
        }
    }

    pub fn from_orientation(o: Orientation) -> DefOrientation {
        match o {
            Orientation::R0 => DefOrientation::N,
            Orientation::R180 => DefOrientation::S,
            Orientation::R90 => DefOrientation::W,
            Orientation::R270 => DefOrientation::E,
            Orientation::MY => DefOrientation::FN,
            Orientation::MX => DefOrientation::FS,
            Orientation::MX90 => DefOrientation::FW,
            Orientation::MY90 => DefOrientation::FE,
        }
    }
}

/// Minimal description of a LEF macro for generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LefShape {
    pub layer: String,
    /// Absolute points in the macro's coordinate system
    pub polygon: Vec<(i64, i64)>,
}

impl LefShape {
    /// Emit as a string in LEF syntax
    pub fn to_string(&self, units_microns: i64) -> String {
        let mut s = String::from("POLYGON ");
        for (i, p) in self.polygon.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(&format!(
                "{} {}",
                (p.0 as f64) / (units_microns as f64),
                (p.1 as f64) / (units_microns as f64)
            ));
        }
        s.push_str(" ;");
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefPoint {
    pub x: i64,
    pub y: i64,
}

impl std::fmt::Display for DefPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "( {} {} )", self.x, self.y)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefDieArea {
    pub points: Vec<DefPoint>,
}

impl std::fmt::Display for DefDieArea {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DIEAREA {} ;",
            self.points
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LefPin {
    pub name: String,
    pub direction: String, // INPUT | OUTPUT | INOUT
    pub shape: LefShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefPin {
    pub name: String,
    pub direction: String, // INPUT | OUTPUT | INOUT
    pub pin_use: String,
    pub layer: String,
    // TODO(sherbst) 2025-11-17: handle polygons?
    pub shape: (DefPoint, DefPoint),
    pub position: DefPoint,
    pub orientation: DefOrientation,
}

impl std::fmt::Display for DefPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "- {} + NET {} + DIRECTION {} + USE {} + LAYER {} {} {} + FIXED {} {} ;",
            self.name,
            self.name,
            self.direction,
            self.pin_use,
            self.layer,
            self.shape.0,
            self.shape.1,
            self.position,
            self.orientation.as_str()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LefComponent {
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub shape: LefShape,
    pub pins: Vec<LefPin>,
}

/// Minimal description of a DEF component placement for generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefComponent {
    pub inst_name: String,
    pub macro_name: String,
    pub x: i64,
    pub y: i64,
    pub orientation: DefOrientation,
}

impl std::fmt::Display for DefComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "- {} {} + PLACED ( {} {} ) {} ;",
            self.inst_name,
            self.macro_name,
            self.x,
            self.y,
            self.orientation.as_str()
        )
    }
}

/// Generate a minimal LEF string from a list of macros.
pub fn generate_lef(macros: &[LefComponent], opts: &LefDefOptions) -> String {
    let mut s = String::new();
    s.push_str("VERSION 5.8 ;\n");
    s.push_str(&format!("DIVIDERCHAR \"{}\" ;\n", opts.divider_char));
    s.push_str(&format!("BUSBITCHARS \"{}\" ;\n", opts.bus_bit_chars));
    s.push_str(&format!(
        "UNITS\n  DATABASE MICRONS {} ;\nEND UNITS\n\n",
        opts.units_microns
    ));
    for m in macros {
        // Basic macro with SIZE as bbox and an OBS POLYGON to capture shape
        s.push_str(&format!("MACRO {}\n", m.name));
        s.push_str("  CLASS BLOCK ;\n");
        s.push_str("  ORIGIN 0 0 ;\n");
        s.push_str(&format!(
            "  SIZE {} BY {} ;\n",
            (m.width as f64) / (opts.units_microns as f64),
            (m.height as f64) / (opts.units_microns as f64)
        ));
        // Pins
        if opts.include_pins {
            for p in &m.pins {
                s.push_str(&format!("  PIN {}\n", p.name));
                s.push_str(&format!("    DIRECTION {} ;\n", p.direction));
                s.push_str("    PORT\n");
                s.push_str(&format!("      LAYER {} ;\n", p.shape.layer));
                let poly = p.shape.to_string(opts.units_microns);
                s.push_str(&format!("      {poly}\n"));
                s.push_str("    END\n");
                s.push_str(&format!("  END {}\n", p.name));
            }
        }
        // Label
        if opts.include_labels {
            // Centered macro label using a false pin
            s.push_str(&format!("  PIN {} ;\n", m.name));
            s.push_str("    PORT\n");
            s.push_str(&format!("      LAYER {} ;\n", m.shape.layer));
            // Rectangle for label anchor; size can be tiny (center of macro)
            let center_x = (m.width as f64) / (opts.units_microns as f64) / 2.0;
            let center_y = (m.height as f64) / (opts.units_microns as f64) / 2.0;
            let min_delta = 1.0 / (opts.units_microns as f64);
            let rect_x1 = center_x - min_delta;
            let rect_x2 = center_x + min_delta;
            let rect_y1 = center_y - min_delta;
            let rect_y2 = center_y + min_delta;
            s.push_str(&format!(
                "      RECT {rect_x1} {rect_y1} {rect_x2} {rect_y2} ;\n",
            ));
            s.push_str("    END\n");
            s.push_str(&format!("  END {}\n", m.name));
        }
        // OBS shape from component polygon and layer
        if opts.include_obstructions {
            let poly = m.shape.to_string(opts.units_microns);
            s.push_str("  OBS\n");
            s.push_str(&format!("    LAYER {} ;\n", m.shape.layer));
            s.push_str(&format!("    {poly}\n"));
            s.push_str("  END\n");
        }
        s.push_str(&format!("END {}\n\n", m.name));
    }
    s.push_str("END LIBRARY\n");
    s
}

/// Generate a minimal DEF string with placed components.
pub fn generate_def(
    design_name: &str,
    die_area: Option<&DefDieArea>,
    pins: &[DefPin],
    components: &[DefComponent],
    opts: &LefDefOptions,
) -> String {
    let mut s = String::new();
    s.push_str("VERSION 5.8 ;\n");
    s.push_str(&format!("DIVIDERCHAR \"{}\" ;\n", opts.divider_char));
    s.push_str(&format!("BUSBITCHARS \"{}\" ;\n", opts.bus_bit_chars));
    s.push_str(&format!("DESIGN {design_name} ;\n"));
    s.push_str(&format!(
        "UNITS DISTANCE MICRONS {} ;\n\n",
        opts.units_microns
    ));

    if let Some(die_area) = die_area {
        s.push_str(&format!("{}\n", die_area));
    }

    if opts.include_pins && !pins.is_empty() {
        s.push_str(&format!("PINS {} ;\n", pins.len()));
        for p in pins {
            s.push_str(&format!("  {}\n", p));
        }
        s.push_str("END PINS\n");
    }

    if !components.is_empty() {
        s.push_str(&format!("COMPONENTS {} ;\n", components.len()));
        for c in components {
            s.push_str(&format!("  {}\n", c));
        }
        s.push_str("END COMPONENTS\n\n");
    }

    s.push_str("END DESIGN\n");

    s
}

/// Write LEF text to a file.
pub fn write_lef_file<P: AsRef<Path>>(
    path: P,
    macros: &[LefComponent],
    opts: &LefDefOptions,
) -> std::io::Result<()> {
    let text = generate_lef(macros, opts);
    fs::write(path, text)
}

/// Write DEF text to a file.
pub fn write_def_file<P: AsRef<Path>>(
    path: P,
    design_name: &str,
    components: &[DefComponent],
    opts: &LefDefOptions,
) -> std::io::Result<()> {
    let text = generate_def(design_name, None, &[], components, opts);
    fs::write(path, text)
}
