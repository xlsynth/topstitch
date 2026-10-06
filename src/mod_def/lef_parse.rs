// SPDX-License-Identifier: Apache-2.0

//! Read macro sizes and pin shapes from a LEF file.
//! Skip other information, such as routing rules and obstructions.

use std::collections::HashSet;

use indexmap::IndexMap;

use super::lefdef::{ImportedPinGeometry, resolve_pin_geometry};
use crate::{Coordinate, IO, LefDefOptions, ModDef, Polygon};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone)]
struct Token {
    text: String,
    line: usize,
    column: usize,
    quoted: bool,
}

impl Token {
    fn is(&self, text: &str) -> bool {
        !self.quoted && self.text.eq_ignore_ascii_case(text)
    }

    fn error(&self, message: impl std::fmt::Display) -> String {
        format!("LEF line {}, column {}: {message}", self.line, self.column)
    }

    fn number(&self) -> Result<f64> {
        let value = self.text.parse::<f64>().ok().filter(|v| v.is_finite());
        value
            .filter(|_| !self.quoted)
            .ok_or_else(|| self.error(format!("expected a finite number, got '{}'", self.text)))
    }

    fn decoded(&self) -> Result<String> {
        let mut chars = self.text.chars();
        let mut result = String::new();
        while let Some(ch) = chars.next() {
            result.push(if ch == '\\' {
                chars
                    .next()
                    .ok_or_else(|| self.error("unterminated escape"))?
            } else {
                ch
            });
        }
        Ok(result)
    }
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn next(&mut self) -> Result<Option<Token>> {
        self.next_context(true)
    }

    fn next_context(&mut self, strings: bool) -> Result<Option<Token>> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.bump();
            }
            if self.peek() != Some('#') {
                break;
            }
            while self.peek().is_some_and(|ch| ch != '\n') {
                self.bump();
            }
        }
        let Some(first) = self.peek() else {
            return Ok(None);
        };
        let mut token = Token {
            text: String::new(),
            line: self.line,
            column: self.column,
            quoted: false,
        };
        if strings && first == '"' {
            token.quoted = true;
            self.bump();
            loop {
                match self.bump() {
                    None => return Err(token.error("unterminated quoted string")),
                    Some('"') => return Ok(Some(token)),
                    Some('\\') => {
                        token.text.push('\\');
                        token.text.push(
                            self.bump()
                                .ok_or_else(|| token.error("unterminated escape"))?,
                        );
                    }
                    Some(ch) => token.text.push(ch),
                }
            }
        }
        if first == ';' {
            token.text.push(self.bump().unwrap());
        } else {
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() || ch == ';' {
                    break;
                }
                token.text.push(self.bump().unwrap());
                if ch == '\\' {
                    token.text.push(
                        self.bump()
                            .ok_or_else(|| token.error("unterminated escape"))?,
                    );
                }
            }
        }
        Ok(Some(token))
    }

    fn required(&mut self) -> Result<Token> {
        self.next()?.ok_or_else(|| {
            format!(
                "LEF line {}, column {}: unexpected end of file",
                self.line, self.column
            )
        })
    }

    fn name(&mut self) -> Result<Token> {
        let token = self.next_context(false)?.ok_or("LEF: missing identifier")?;
        if token.is(";") {
            return Err(token.error("expected an identifier"));
        }
        Ok(token)
    }

    fn end_name(&mut self, name: &Token) -> Result<()> {
        let end = self.name()?;
        if end.text != name.text {
            return Err(end.error(format!("expected END {}, got END {}", name.text, end.text)));
        }
        Ok(())
    }

    fn statement(&mut self, begin: &Token) -> Result<Vec<Token>> {
        if begin.is("PROPERTY") {
            self.properties(begin)?;
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        loop {
            let token = self
                .next()?
                .ok_or_else(|| begin.error(format!("unterminated {} statement", begin.text)))?;
            if token.is(";") {
                return Ok(values);
            }
            values.push(token);
        }
    }

    fn properties(&mut self, begin: &Token) -> Result<()> {
        loop {
            let name = self
                .next_context(false)?
                .ok_or_else(|| begin.error("unterminated PROPERTY"))?;
            if name.is(";") {
                return Ok(());
            }
            let value = self.required()?;
            if value.is(";") {
                return Err(name.error("missing PROPERTY value"));
            }
        }
    }

    fn extension(&mut self, begin: &Token) -> Result<()> {
        while let Some(token) = self.next()? {
            if token.is("ENDEXT") {
                return Ok(());
            }
        }
        Err(begin.error("unterminated BEGINEXT"))
    }

    fn skip_named(&mut self, name: &Token, kind: &str) -> Result<()> {
        while let Some(token) = self.next()? {
            if token.is("END") {
                let end = self.name()?;
                if end.text == name.text || (name.is(kind) && end.is(kind)) {
                    return Ok(());
                }
            } else if token.is("BEGINEXT") {
                self.extension(&token)?;
            } else if (kind == "NONDEFAULTRULE" && (token.is("LAYER") || token.is("VIA")))
                || (kind == "ARRAY" && token.is("FLOORPLAN"))
            {
                let nested = self.name()?;
                self.skip_named(&nested, &token.text.to_ascii_uppercase())?;
            } else if kind == "ARRAY" && token.is("DEFAULTCAP") {
                self.skip_named(&token, "DEFAULTCAP")?;
            } else if kind == "PROPERTYDEFINITIONS" && !token.is(";") {
                self.name()?;
                self.statement(&token)?;
            } else if !token.is(";") {
                self.statement(&token)?;
            }
        }
        Err(name.error(format!("missing END {}", name.text)))
    }

    fn skip_bare(&mut self, begin: &Token) -> Result<()> {
        while let Some(token) = self.next()? {
            if token.is("END") {
                return Ok(());
            }
            if token.is("BEGINEXT") {
                self.extension(&token)?;
            } else {
                self.statement(&token)?;
            }
        }
        Err(begin.error(format!("unterminated {}", begin.text)))
    }
}

struct ParsedGeometry {
    kind: Token,
    layer: Option<Token>,
    values: Vec<Token>,
}

struct ParsedPin {
    name: Token,
    direction: Option<Token>,
    pin_use: Option<Token>,
    geometries: Vec<ParsedGeometry>,
}

struct ParsedMacro {
    name: Token,
    bus_chars: (char, char),
    size: Option<(Token, Token)>,
    origin: Option<(Token, Vec<Token>)>,
    pins: Vec<ParsedPin>,
}

fn parse_port(lexer: &mut Lexer<'_>, begin: &Token) -> Result<Vec<ParsedGeometry>> {
    let mut geometries = Vec::new();
    let mut layer = None;
    while let Some(keyword) = lexer.next()? {
        if keyword.is("END") {
            return Ok(geometries);
        }
        if keyword.is("BEGINEXT") {
            lexer.extension(&keyword)?;
            continue;
        }
        if keyword.is("LAYER") {
            let name = lexer
                .next_context(false)?
                .ok_or_else(|| keyword.error("unterminated LAYER statement"))?;
            if name.is(";") {
                layer = None;
            } else {
                layer = Some(name);
                lexer.statement(&keyword)?;
            }
            continue;
        }
        let values = lexer.statement(&keyword)?;
        if ["RECT", "POLYGON", "PATH", "VIA"]
            .iter()
            .any(|k| keyword.is(k))
        {
            geometries.push(ParsedGeometry {
                layer: if keyword.is("VIA") {
                    None
                } else {
                    layer.clone()
                },
                kind: keyword,
                values,
            });
        }
    }
    Err(begin.error("unterminated PORT"))
}

fn parse_pin(lexer: &mut Lexer<'_>) -> Result<ParsedPin> {
    let name = lexer.name()?;
    let mut pin = ParsedPin {
        name,
        direction: None,
        pin_use: None,
        geometries: Vec::new(),
    };
    while let Some(keyword) = lexer.next()? {
        if keyword.is("END") {
            lexer.end_name(&pin.name)?;
            return Ok(pin);
        }
        if keyword.is("PORT") {
            pin.geometries.extend(parse_port(lexer, &keyword)?);
        } else if keyword.is("BEGINEXT") {
            lexer.extension(&keyword)?;
        } else if !keyword.is(";") {
            let values = lexer.statement(&keyword)?;
            if keyword.is("DIRECTION") || keyword.is("USE") {
                let value = values
                    .into_iter()
                    .next()
                    .ok_or_else(|| keyword.error("missing pin attribute value"))?;
                if keyword.is("DIRECTION") {
                    pin.direction = Some(value);
                } else {
                    pin.pin_use = Some(value);
                }
            }
        }
    }
    Err(pin.name.error(format!("missing END {}", pin.name.text)))
}

fn parse_macro(
    lexer: &mut Lexer<'_>,
    bus_chars: (char, char),
    skip: &HashSet<String>,
) -> Result<ParsedMacro> {
    let name = lexer.name()?;
    let mut block = ParsedMacro {
        name,
        bus_chars,
        size: None,
        origin: None,
        pins: Vec::new(),
    };
    while let Some(keyword) = lexer.next()? {
        if keyword.is("END") {
            lexer.end_name(&block.name)?;
            return Ok(block);
        }
        if keyword.is("PIN") {
            block.pins.push(parse_pin(lexer)?);
        } else if keyword.is("OBS") || keyword.is("DENSITY") {
            lexer.skip_bare(&keyword)?;
        } else if keyword.is("TIMING") {
            lexer.skip_named(&keyword, &keyword.text.to_ascii_uppercase())?;
        } else if keyword.is("BEGINEXT") {
            lexer.extension(&keyword)?;
        } else if skip.contains(&keyword.text.to_ascii_uppercase()) {
            lexer.skip_named(&keyword, &keyword.text.to_ascii_uppercase())?;
        } else if !keyword.is(";") {
            let values = lexer.statement(&keyword)?;
            if keyword.is("SIZE") {
                if values.len() != 3 || !values[1].is("BY") {
                    return Err(keyword.error("expected SIZE width BY height ;"));
                }
                block.size = Some((values[0].clone(), values[2].clone()));
            } else if keyword.is("ORIGIN") {
                block.origin = Some((keyword, values));
            }
        }
    }
    Err(block.name.error(format!("missing END {}", block.name.text)))
}

fn parse(lef: &str, opts: &LefDefOptions) -> Result<Vec<ParsedMacro>> {
    let mut lexer = Lexer::new(lef);
    let mut macros = Vec::new();
    let mut bus_chars = opts.open_close_chars();
    let skip: HashSet<_> = opts
        .skip_lef_sections
        .iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();
    while let Some(keyword) = lexer.next()? {
        let kind = keyword.text.to_ascii_uppercase();
        if keyword.is("MACRO") {
            macros.push(parse_macro(&mut lexer, bus_chars, &skip)?);
        } else if keyword.is("BUSBITCHARS") {
            let values = lexer.statement(&keyword)?;
            let value = values
                .first()
                .ok_or_else(|| keyword.error("missing BUSBITCHARS value"))?;
            let chars: Vec<_> = value.decoded()?.chars().collect();
            if values.len() != 1 || chars.len() != 2 || chars[0] == chars[1] {
                return Err(value.error("BUSBITCHARS must contain two distinct characters"));
            }
            bus_chars = (chars[0], chars[1]);
        } else if ["LAYER", "VIA", "VIARULE", "SITE", "NONDEFAULTRULE", "ARRAY"]
            .contains(&kind.as_str())
        {
            let name = lexer.name()?;
            lexer.skip_named(&name, &kind)?;
        } else if [
            "UNITS",
            "PROPERTYDEFINITIONS",
            "SPACING",
            "IRDROP",
            "NOISETABLE",
            "CORRECTIONTABLE",
        ]
        .contains(&kind.as_str())
        {
            lexer.skip_named(&keyword, &kind)?;
        } else if keyword.is("BEGINEXT") {
            lexer.extension(&keyword)?;
        } else if keyword.is("END") {
            let end = lexer.required()?;
            if !end.is("LIBRARY") {
                return Err(end.error("expected END LIBRARY"));
            }
            break;
        } else if skip.contains(&kind) {
            lexer.skip_named(&keyword, &kind)?;
        } else if !keyword.is(";") {
            lexer.statement(&keyword)?;
        }
    }
    Ok(macros)
}

fn pin_name(token: &Token, (open, close): (char, char)) -> Result<(String, usize, bool)> {
    let mut chars = token.text.chars();
    let mut decoded = Vec::new();
    while let Some(ch) = chars.next() {
        decoded.push(if ch == '\\' {
            (
                chars
                    .next()
                    .ok_or_else(|| token.error("unterminated escape"))?,
                true,
            )
        } else {
            (ch, false)
        });
    }
    if decoded.last() == Some(&(close, false))
        && let Some(index) = decoded.iter().rposition(|c| *c == (open, false))
    {
        let digits: String = decoded[index + 1..decoded.len() - 1]
            .iter()
            .map(|c| c.0)
            .collect();
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let bit = digits
                .parse()
                .map_err(|_| token.error("bus index is too large"))?;
            let base: String = decoded[..index].iter().map(|c| c.0).collect();
            if base.is_empty() {
                return Err(token.error("empty bus name"));
            }
            return Ok((base, bit, true));
        }
    }
    Ok((decoded.iter().map(|c| c.0).collect(), 0, false))
}

fn scale(value: f64, units: i64, source: &Token) -> Result<i64> {
    let scaled = (value * units as f64).round();
    // i64::MAX rounds up to 2^63 as f64, so use an exclusive upper bound.
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled >= -(i64::MIN as f64) {
        return Err(source.error("coordinate is outside the supported integer range"));
    }
    Ok(scaled as i64)
}

// Parentheses delimit points only in coordinate contexts. In names they may
// be literal characters or the declared bus delimiters, as in data(0).
fn coordinate_pairs(values: &[Token], source: &Token) -> Result<Vec<(f64, f64)>> {
    let fields: Vec<_> = values
        .iter()
        .flat_map(|token| {
            if token.quoted {
                return vec![token.clone()];
            }
            token
                .text
                .replace('(', " ( ")
                .replace(')', " ) ")
                .split_whitespace()
                .map(|text| Token {
                    text: text.to_string(),
                    ..token.clone()
                })
                .collect()
        })
        .collect();
    let mut points = Vec::new();
    let mut cursor = 0;
    while cursor < fields.len() {
        let parenthesized = fields[cursor].is("(");
        cursor += usize::from(parenthesized);
        if cursor + 1 >= fields.len() {
            return Err(source.error("expected an x/y coordinate pair"));
        }
        let point = (fields[cursor].number()?, fields[cursor + 1].number()?);
        cursor += 2;
        if parenthesized {
            if fields.get(cursor).is_none_or(|token| !token.is(")")) {
                return Err(source.error("expected ')' after coordinate pair"));
            }
            cursor += 1;
        }
        points.push(point);
    }
    Ok(points)
}

impl ParsedGeometry {
    fn convert(self, origin: (f64, f64), units: i64) -> Result<ImportedPinGeometry> {
        if self.kind.is("VIA")
            || self.kind.is("PATH")
            || self.values.iter().any(|v| v.is("ITERATE"))
        {
            return Ok(ImportedPinGeometry::Unsupported(
                self.kind.error(format!("{} geometry", self.kind.text)),
            ));
        }
        let layer = self
            .layer
            .ok_or_else(|| self.kind.error("pin geometry missing LAYER"))?
            .decoded()?;
        let mut values = self.values.as_slice();
        if values.first().is_some_and(|v| v.is("MASK")) {
            let mask = values
                .get(1)
                .ok_or_else(|| self.kind.error("missing MASK number"))?;
            mask.text
                .parse::<u64>()
                .map_err(|_| mask.error("invalid MASK number"))?;
            values = &values[2..];
        }
        let points = coordinate_pairs(values, &self.kind)?;
        let points = if self.kind.is("RECT") {
            if points.len() != 2 {
                return Err(self.kind.error("RECT requires two coordinate pairs"));
            }
            let ((x1, y1), (x2, y2)) = (points[0], points[1]);
            vec![(x1, y1), (x1, y2), (x2, y2), (x2, y1)]
        } else {
            if points.len() < 3 {
                return Err(self
                    .kind
                    .error("POLYGON requires at least three coordinate pairs"));
            }
            points
        };
        let points = points
            .into_iter()
            .map(|(x, y)| {
                Ok(Coordinate {
                    x: scale(x + origin.0, units, &self.kind)?,
                    y: scale(y + origin.1, units, &self.kind)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ImportedPinGeometry::Polygon {
            layer,
            polygon: Polygon::new(points),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PinDirection {
    Input,
    Output,
    InOut,
}

struct ParsedPort {
    source: Token,
    is_bus: bool,
    direction: Option<PinDirection>,
    pin_use: String,
    bits: IndexMap<usize, Vec<ParsedGeometry>>,
}

fn construct(block: ParsedMacro, opts: &LefDefOptions) -> Result<ModDef> {
    let module = ModDef::new(block.name.decoded()?);
    if let Some((width, height)) = block.size {
        let width = scale(width.number()?, opts.units_microns, &width)?;
        let height = scale(height.number()?, opts.units_microns, &height)?;
        if width <= 0 || height <= 0 {
            return Err(block
                .name
                .error("SIZE must remain positive after conversion"));
        }
        module.set_width_height(width, height);
    }
    let origin = match block.origin {
        Some((source, values)) => {
            let points = coordinate_pairs(&values, &source)?;
            if points.len() != 1 {
                return Err(source.error("ORIGIN requires one coordinate pair"));
            }
            points[0]
        }
        None => (0.0, 0.0),
    };
    let mut ports: IndexMap<String, ParsedPort> = IndexMap::new();
    for pin in block.pins {
        let (name, bit, is_bus) = pin_name(&pin.name, block.bus_chars)?;
        if opts.ignore_pin_names.contains(&name) {
            continue;
        }
        let pin_use = pin
            .pin_use
            .as_ref()
            .map_or("SIGNAL", |t| t.text.as_str())
            .to_ascii_uppercase();
        if opts
            .skip_pin_uses
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&pin_use))
        {
            continue;
        }
        let direction = pin
            .direction
            .as_ref()
            .map(|value| match value.text.to_ascii_uppercase().as_str() {
                "INPUT" => Ok(PinDirection::Input),
                "OUTPUT" => Ok(PinDirection::Output),
                "INOUT" | "FEEDTHRU" => Ok(PinDirection::InOut),
                _ => Err(value.error("unsupported pin DIRECTION")),
            })
            .transpose()?;
        let port = ports.entry(name).or_insert_with(|| ParsedPort {
            source: pin.name.clone(),
            is_bus,
            direction,
            pin_use: pin_use.clone(),
            bits: IndexMap::new(),
        });
        if port.is_bus != is_bus
            || (port.direction.is_some() && direction.is_some() && port.direction != direction)
            || port.pin_use != pin_use
        {
            return Err(pin
                .name
                .error("inconsistent scalar/bus identity, DIRECTION, or USE"));
        }
        port.direction = port.direction.or(direction);
        if port.bits.insert(bit, pin.geometries).is_some() {
            return Err(pin.name.error("duplicate pin bit"));
        }
    }
    for (name, port) in ports {
        let max_bit = *port.bits.keys().max().unwrap();
        let width = max_bit
            .checked_add(1)
            .ok_or_else(|| port.source.error("bus width overflow"))?;
        if port.bits.len() != width {
            return Err(port
                .source
                .error("pin bus indices must be contiguous from zero"));
        }
        module.add_port(
            &name,
            match port.direction.unwrap_or(PinDirection::InOut) {
                PinDirection::Input => IO::Input(width),
                PinDirection::Output => IO::Output(width),
                PinDirection::InOut => IO::InOut(width),
            },
        );
        for (bit, geometries) in port.bits {
            let selected_layer = opts
                .pin_layer_selections
                .get(&(name.clone(), bit))
                .map(String::as_str);
            let geometries = geometries.into_iter().filter_map(|geometry| {
                if let Some(layer) = &geometry.layer {
                    let layer = match layer.decoded() {
                        Ok(layer) => layer,
                        Err(error) => return Some(Err(error)),
                    };
                    if opts
                        .valid_pin_layers
                        .as_ref()
                        .is_some_and(|layers| !layers.contains(&layer))
                        || selected_layer.is_some_and(|selected| selected != layer)
                    {
                        return None;
                    }
                }
                Some(geometry.convert(origin, opts.units_microns))
            });
            let pin =
                resolve_pin_geometry(geometries, opts.multiple_pin_shapes_policy, selected_layer)
                    .map_err(|error| format!("LEF pin '{name}' bit {bit}: {error}"))?;
            if let Some(pin) = pin {
                module.place_pin(&name, bit, pin);
            }
        }
    }
    Ok(module)
}

pub(crate) fn mod_defs_from_lef(lef: &str, opts: &LefDefOptions) -> Vec<ModDef> {
    let import = || -> Result<Vec<ModDef>> {
        if opts.units_microns <= 0 {
            return Err("LEF: units_microns must be positive".to_string());
        }
        parse(lef, opts)?
            .into_iter()
            .map(|block| construct(block, opts))
            .collect()
    };
    import().unwrap_or_else(|error| panic!("{error}"))
}
