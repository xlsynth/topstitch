// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use indexmap::IndexMap;

use crate::{Coordinate, LefDefOptions, ModDef, PhysicalPin, Polygon};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinDirection {
    Input,
    Output,
    InOut,
}

impl PinDirection {
    fn from_lef_token(token: &str) -> Option<Self> {
        match token {
            "INPUT" => Some(PinDirection::Input),
            "OUTPUT" => Some(PinDirection::Output),
            "INOUT" => Some(PinDirection::InOut),
            _ => None,
        }
    }
}

#[derive(Default)]
struct ParsedPort {
    direction: Option<PinDirection>,
    pin_use: Option<String>,
    bits: IndexMap<usize, Option<ParsedPinGeometry>>,
}

struct ParsedPinGeometry {
    layer: String,
    polygon_um: Vec<(f64, f64)>,
}

struct ParsedMacro {
    name: String,
    width_um: Option<f64>,
    height_um: Option<f64>,
    pins: IndexMap<String, ParsedPort>,
}

fn clean_lef_token(token: &str) -> &str {
    token.trim_matches(&[';', '"'][..])
}

fn parse_pin_name(name: &str, open_char: char, close_char: char) -> (String, usize) {
    if name.ends_with(close_char)
        && let Some(open_pos) = name.rfind(open_char)
        && open_pos + 1 < name.len()
    {
        let base = &name[..open_pos];
        let index_str = &name[open_pos + 1..name.len() - 1];
        if let Ok(index) = index_str.parse::<usize>() {
            return (base.to_string(), index);
        }
    }
    (name.to_string(), 0)
}

fn parse_lef_numbers(tokens: &[&str], line: &str) -> Vec<f64> {
    tokens
        .iter()
        .map(|t| clean_lef_token(t))
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<f64>()
                .unwrap_or_else(|_| panic!("Invalid LEF number '{t}' in line: '{line}'"))
        })
        .collect()
}

fn parse_lef_macros(
    lef: &str,
    open_char: char,
    close_char: char,
    skip_sections: &HashSet<String>,
    valid_pin_layers: Option<&HashSet<String>>,
) -> Vec<ParsedMacro> {
    let mut macros = Vec::new();
    let mut current_macro: Option<ParsedMacro> = None;
    let mut current_pin: Option<(String, usize)> = None;
    let mut in_port = false;
    let mut current_layer: Option<String> = None;
    let mut in_skipped_section: Option<String> = None;

    for raw_line in lef.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        let first_token = tokens[0].to_ascii_uppercase();
        if let Some(section_name) = in_skipped_section.as_ref() {
            if first_token == "END"
                && tokens.get(1).map(|t| t.to_ascii_uppercase()).as_deref()
                    == Some(section_name.as_str())
            {
                in_skipped_section = None;
            }
            continue;
        }
        if skip_sections.contains(&first_token) {
            in_skipped_section = Some(first_token);
            continue;
        }
        match first_token.as_str() {
            "MACRO" => {
                if let Some(m) = current_macro.take() {
                    macros.push(m);
                }
                let name = tokens
                    .get(1)
                    .map(|t| clean_lef_token(t).to_string())
                    .unwrap_or_else(|| panic!("Invalid LEF MACRO line: '{line}'"));
                current_macro = Some(ParsedMacro {
                    name,
                    width_um: None,
                    height_um: None,
                    pins: IndexMap::new(),
                });
                current_pin = None;
                in_port = false;
                current_layer = None;
            }
            "PIN" => {
                let pin_name = tokens
                    .get(1)
                    .map(|t| clean_lef_token(t).to_string())
                    .unwrap_or_else(|| panic!("Invalid LEF PIN line: '{line}'"));
                let (base_name, bit_idx) = parse_pin_name(&pin_name, open_char, close_char);
                let m = current_macro
                    .as_mut()
                    .unwrap_or_else(|| panic!("LEF pin '{pin_name}' defined outside of a MACRO"));
                let entry = m
                    .pins
                    .entry(base_name.clone())
                    .or_insert_with(|| ParsedPort {
                        direction: None,
                        pin_use: None,
                        bits: IndexMap::new(),
                    });
                if entry.bits.contains_key(&bit_idx) {
                    panic!("LEF pin '{}' repeats bit {}", base_name, bit_idx);
                }
                entry.bits.insert(bit_idx, None);
                current_pin = Some((base_name, bit_idx));
                in_port = false;
                current_layer = None;
            }
            "PORT" => {
                in_port = true;
                current_layer = None;
            }
            "END" => {
                if in_port && tokens.len() == 1 {
                    in_port = false;
                    current_layer = None;
                }
            }
            "DIRECTION" => {
                let m = current_macro
                    .as_mut()
                    .unwrap_or_else(|| panic!("LEF DIRECTION defined outside of a MACRO"));
                let (base_name, _) = current_pin
                    .as_ref()
                    .unwrap_or_else(|| panic!("LEF DIRECTION defined outside of a PIN"));
                if let Some(token) = tokens.get(1) {
                    let dir_token = clean_lef_token(token).to_ascii_uppercase();
                    if let Some(dir) = PinDirection::from_lef_token(dir_token.as_str()) {
                        let entry = m.pins.get_mut(base_name).unwrap();
                        match entry.direction {
                            Some(existing) => {
                                assert_eq!(
                                    existing, dir,
                                    "Mismatched pin directions for LEF pin '{}'",
                                    base_name
                                );
                            }
                            None => entry.direction = Some(dir),
                        }
                    }
                }
            }
            "USE" => {
                let m = current_macro
                    .as_mut()
                    .unwrap_or_else(|| panic!("LEF USE defined outside of a MACRO"));
                let (base_name, _) = current_pin
                    .as_ref()
                    .unwrap_or_else(|| panic!("LEF USE defined outside of a PIN"));
                if let Some(token) = tokens.get(1) {
                    let pin_use = clean_lef_token(token).to_ascii_uppercase();
                    let entry = m.pins.get_mut(base_name).unwrap();
                    match entry.pin_use.as_deref() {
                        Some(existing) => {
                            assert_eq!(
                                existing, pin_use,
                                "Mismatched pin USE values for LEF pin '{}'",
                                base_name
                            );
                        }
                        None => entry.pin_use = Some(pin_use),
                    }
                }
            }
            "LAYER" => {
                if in_port {
                    let layer = tokens
                        .get(1)
                        .map(|t| clean_lef_token(t).to_string())
                        .unwrap_or_else(|| panic!("Invalid LEF LAYER line: '{line}'"));
                    current_layer = Some(layer);
                }
            }
            "RECT" | "POLYGON" => {
                if in_port {
                    let m = current_macro
                        .as_mut()
                        .unwrap_or_else(|| panic!("LEF {first_token} defined outside of a MACRO"));
                    let (base_name, bit_idx) = current_pin
                        .as_ref()
                        .unwrap_or_else(|| panic!("LEF {first_token} defined outside of a PIN"));
                    let entry = m.pins.get_mut(base_name).unwrap();
                    let slot = entry.bits.get_mut(bit_idx).unwrap_or_else(|| {
                        panic!(
                            "LEF {first_token} defined for unknown bit {} of pin '{}'",
                            bit_idx, base_name
                        )
                    });
                    if slot.is_some() {
                        continue;
                    }
                    let Some(layer) = current_layer.clone() else {
                        panic!("LEF {first_token} for pin '{}' missing LAYER", base_name);
                    };
                    if let Some(valid_layers) = valid_pin_layers
                        && !valid_layers.contains(&layer)
                    {
                        continue;
                    }
                    let numbers = parse_lef_numbers(&tokens[1..], line);
                    let points = if first_token == "RECT" {
                        if numbers.len() != 4 {
                            panic!("Invalid LEF RECT line: '{line}'");
                        }
                        let x1 = numbers[0];
                        let y1 = numbers[1];
                        let x2 = numbers[2];
                        let y2 = numbers[3];
                        vec![(x1, y1), (x1, y2), (x2, y2), (x2, y1)]
                    } else {
                        if numbers.len() < 6 || !numbers.len().is_multiple_of(2) {
                            panic!("Invalid LEF POLYGON line: '{line}'");
                        }
                        let mut points = Vec::new();
                        for pair in numbers.chunks(2) {
                            points.push((pair[0], pair[1]));
                        }
                        points
                    };
                    *slot = Some(ParsedPinGeometry {
                        layer,
                        polygon_um: points,
                    });
                }
            }
            "SIZE" => {
                let m = current_macro
                    .as_mut()
                    .unwrap_or_else(|| panic!("LEF SIZE defined outside of a MACRO"));
                let width_token = tokens
                    .get(1)
                    .map(|t| clean_lef_token(t))
                    .unwrap_or_else(|| panic!("Invalid LEF SIZE line: '{line}'"));
                let by_token = tokens
                    .get(2)
                    .unwrap_or_else(|| panic!("Invalid LEF SIZE line: '{line}'"));
                let height_token = tokens
                    .get(3)
                    .map(|t| clean_lef_token(t))
                    .unwrap_or_else(|| panic!("Invalid LEF SIZE line: '{line}'"));
                assert_eq!(
                    by_token.to_ascii_uppercase(),
                    "BY",
                    "Invalid LEF SIZE line: '{line}'"
                );
                let width_um = width_token
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("Invalid LEF size value: '{width_token}'"));
                let height_um = height_token
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("Invalid LEF size value: '{height_token}'"));
                m.width_um = Some(width_um);
                m.height_um = Some(height_um);
            }
            _ => {}
        }
    }

    if let Some(m) = current_macro.take() {
        macros.push(m);
    }

    macros
}

pub(crate) fn mod_defs_from_lef(lef: &str, opts: &LefDefOptions) -> Vec<ModDef> {
    let (open_char, close_char) = opts.open_close_chars();
    let skip_sections = opts
        .skip_lef_sections
        .iter()
        .map(|name| name.to_ascii_uppercase())
        .collect::<HashSet<_>>();
    let skip_pin_uses = opts
        .skip_pin_uses
        .iter()
        .map(|u| u.to_ascii_uppercase())
        .collect::<HashSet<_>>();
    let macros = parse_lef_macros(
        lef,
        open_char,
        close_char,
        &skip_sections,
        opts.valid_pin_layers.as_ref(),
    );
    let mut mod_defs = Vec::new();

    for m in macros {
        let mod_def = ModDef::new(&m.name);

        if let Some(width_um) = m.width_um
            && let Some(height_um) = m.height_um
        {
            let width = (width_um * opts.units_microns as f64).round() as i64;
            let height = (height_um * opts.units_microns as f64).round() as i64;
            mod_def.set_width_height(width, height);
        }

        for (name, parsed_port) in m.pins {
            if opts.ignore_pin_names.contains(&name) {
                continue;
            }
            let pin_use = parsed_port
                .pin_use
                .as_deref()
                .unwrap_or("SIGNAL")
                .to_ascii_uppercase();
            if skip_pin_uses.contains(&pin_use) {
                continue;
            }
            let max_bit = parsed_port
                .bits
                .keys()
                .copied()
                .max()
                .unwrap_or_else(|| panic!("LEF pin '{}' has no bits", name));
            for bit in 0..=max_bit {
                if !parsed_port.bits.contains_key(&bit) {
                    panic!("LEF pin '{}' has non-contiguous bit indices", name);
                }
            }
            let width = max_bit + 1;

            let io = match parsed_port.direction.unwrap_or(PinDirection::InOut) {
                PinDirection::Input => crate::IO::Input(width),
                PinDirection::Output => crate::IO::Output(width),
                PinDirection::InOut => crate::IO::InOut(width),
            };
            mod_def.add_port(&name, io);

            for (bit, geom) in parsed_port.bits {
                let Some(geom) = geom else {
                    continue;
                };
                let points = geom
                    .polygon_um
                    .iter()
                    .map(|(x, y)| Coordinate {
                        x: (x * opts.units_microns as f64).round() as i64,
                        y: (y * opts.units_microns as f64).round() as i64,
                    })
                    .collect::<Vec<_>>();
                if points.len() < 3 {
                    panic!("LEF pin '{}' has an invalid shape", name);
                }
                let polygon = Polygon::new(points);
                mod_def.place_pin(&name, bit, PhysicalPin::new(&geom.layer, polygon));
            }
        }

        mod_defs.push(mod_def);
    }

    mod_defs
}
