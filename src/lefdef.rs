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
}

impl Default for LefDefOptions {
    fn default() -> Self {
        LefDefOptions {
            divider_char: "/".to_string(),
            bus_bit_chars: "[]".to_string(),
            units_microns: 1,
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
    /// Emit this shape's polygon in LEF syntax
    pub fn to_lef_polygon(&self, units_microns: i64) -> String {
        let mut s = String::from("POLYGON ( ");
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
        s.push_str(" ) ;");
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LefPin {
    pub name: String,
    pub direction: String, // INPUT | OUTPUT | INOUT
    pub shape: LefShape,
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
        for p in &m.pins {
            s.push_str(&format!("  PIN {}\n", p.name));
            s.push_str(&format!("    DIRECTION {} ;\n", p.direction));
            s.push_str("    PORT\n");
            s.push_str(&format!("      LAYER {} ;\n", p.shape.layer));
            let poly = p.shape.to_lef_polygon(opts.units_microns);
            s.push_str(&format!("      {poly}\n"));
            s.push_str("    END\n");
            s.push_str(&format!("  END {}\n", p.name));
        }
        // OBS shape from component polygon and layer
        let poly = m.shape.to_lef_polygon(opts.units_microns);
        s.push_str("  OBS\n");
        s.push_str(&format!("    LAYER {} ;\n", m.shape.layer));
        s.push_str(&format!("      {poly}\n"));
        s.push_str("  END\n");
        s.push_str("END ");
        s.push_str(&m.name);
        s.push_str("\n\n");
    }
    s.push_str("END LIBRARY\n");
    s
}

/// Generate a minimal DEF string with placed components.
pub fn generate_def(
    design_name: &str,
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

    s.push_str(&format!("COMPONENTS {} ;\n", components.len()));
    for c in components {
        s.push_str(&format!(
            "  - {} {} + PLACED ( {} {} ) {} ;\n",
            c.inst_name,
            c.macro_name,
            c.x,
            c.y,
            c.orientation.as_str()
        ));
    }
    s.push_str("END COMPONENTS\n\n");
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
    let text = generate_def(design_name, components, opts);
    fs::write(path, text)
}
