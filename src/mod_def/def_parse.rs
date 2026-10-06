// SPDX-License-Identifier: Apache-2.0

//! Read a module's outline and external pins from a DEF file.
//! Skip other information, such as internal components and routing.

use indexmap::IndexMap;
use std::collections::HashSet;

use super::lefdef::{ImportedPinGeometry, resolve_pin_geometry};
use crate::{Coordinate, IO, LefDefOptions, ModDef, MultiplePinShapesPolicy, PhysicalPin, Polygon};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
struct Token {
    text: String,
    line: usize,
    column: usize,
    quoted: bool,
}

impl Token {
    fn error(&self, message: impl std::fmt::Display) -> String {
        format!("DEF line {}, column {}: {message}", self.line, self.column)
    }

    fn is(&self, value: &str) -> bool {
        !self.quoted && self.text.eq_ignore_ascii_case(value)
    }

    fn integer(&self) -> Result<i64> {
        self.text
            .parse()
            .map_err(|_| self.error(format!("expected an integer, got '{}'", self.text)))
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

    fn whitespace(&mut self) {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.bump();
            }
            if self.peek() != Some('#') {
                break;
            }
            while self.peek().is_some_and(|c| c != '\n') {
                self.bump();
            }
        }
    }

    // Identifiers have a different lexical context from quoted property values:
    // punctuation (even a leading quote) is legal in an identifier.
    fn name(&mut self) -> Result<Token> {
        self.whitespace();
        let mut token = Token {
            text: String::new(),
            line: self.line,
            column: self.column,
            quoted: false,
        };
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                break;
            }
            token.text.push(self.bump().unwrap());
        }
        if token.text.is_empty() {
            return Err(token.error("expected an identifier"));
        }
        Ok(token)
    }

    fn next(&mut self) -> Result<Option<Token>> {
        self.next_context(true, true)
    }

    fn next_context(&mut self, strings: bool, compact_coordinates: bool) -> Result<Option<Token>> {
        self.whitespace();
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
                    Some('"') => break,
                    Some('\\') => {
                        let ch = self
                            .bump()
                            .ok_or_else(|| token.error("unterminated escape"))?;
                        token.text.push(ch);
                    }
                    Some(ch) => token.text.push(ch),
                }
            }
            return Ok(Some(token));
        }
        if matches!(first, '(' | ')' | ';' | '+') {
            token.text.push(self.bump().unwrap());
            return Ok(Some(token));
        }
        let numeric = first.is_ascii_digit() || first == '-' || first == '*';
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                break;
            }
            // Accept compact coordinate syntax such as (10 20), while keeping
            // punctuation embedded in ordinary identifiers as part of the name.
            if compact_coordinates && numeric && matches!(ch, '(' | ')' | ';' | '+') {
                break;
            }
            if compact_coordinates
                && ch == ';'
                && self.source[self.offset + 1..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
            {
                break;
            }
            token.text.push(self.bump().unwrap());
            if ch == '\\' {
                let escaped = self
                    .bump()
                    .ok_or_else(|| token.error("unterminated escape"))?;
                token.text.push(escaped);
            }
        }
        Ok(Some(token))
    }

    fn required(&mut self) -> Result<Token> {
        self.next()?.ok_or_else(|| {
            format!(
                "DEF line {}, column {}: unexpected end of file",
                self.line, self.column
            )
        })
    }

    fn expect(&mut self, expected: &str) -> Result<Token> {
        let token = self.required()?;
        if !token.is(expected) {
            return Err(token.error(format!("expected {expected}, got '{}'", token.text)));
        }
        Ok(token)
    }

    // Extensions are opaque text, not DEF records. In particular semicolons and
    // END PINS inside an extension must not affect the surrounding parser.
    fn extension(&mut self, begin: &Token) -> Result<()> {
        self.whitespace();
        if self.peek() != Some('"') {
            return Err(begin.error("BEGINEXT requires a quoted tag"));
        }
        self.required()?;
        let mut quoted = false;
        let mut escaped = false;
        let mut boundary = true;
        loop {
            if boundary && !quoted && self.source[self.offset..].starts_with("ENDEXT") {
                let end = self.offset + "ENDEXT".len();
                if self.source[end..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || c == ';' || c == '+')
                {
                    for _ in 0..6 {
                        self.bump();
                    }
                    return Ok(());
                }
            }
            let ch = self
                .bump()
                .ok_or_else(|| begin.error("unterminated BEGINEXT"))?;
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = !quoted;
            }
            boundary = ch.is_whitespace();
        }
    }

    fn statement(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.required()?;
            if token.is(";") {
                return Ok(tokens);
            }
            if token.is("BEGINEXT") {
                self.extension(&token)?;
            } else {
                tokens.push(token);
            }
        }
    }

    fn skip_statement(&mut self) -> Result<()> {
        loop {
            let token = self.required()?;
            if token.is(";") {
                return Ok(());
            }
            if token.is("BEGINEXT") {
                self.extension(&token)?;
            }
        }
    }

    fn skip_section(&mut self, section: &Token) -> Result<()> {
        loop {
            let token = self
                .next_context(false, false)?
                .ok_or_else(|| section.error("unterminated section"))?;
            if token.is("END") {
                self.expect(&section.text)?;
                return Ok(());
            }
            if token.is("BEGINEXT") {
                self.extension(&token)?;
            } else {
                // The record's name can itself start with a quote.
                if token.is("-") {
                    self.name()?;
                }
                self.skip_record(section.is("PROPERTYDEFINITIONS"))?;
            }
        }
    }

    fn skip_record(&mut self, property_definition: bool) -> Result<()> {
        let mut after_plus = false;
        let mut string_value = false;
        loop {
            // In skipped records, names can begin with quotes or digits and
            // contain punctuation. Only the known string-value contexts invoke
            // quote parsing. DEF requires a separate semicolon token.
            let token = self
                .next_context(string_value, false)?
                .ok_or_else(|| format!("DEF line {}: unterminated statement", self.line))?;
            string_value = false;
            if token.is(";") {
                return Ok(());
            }
            if after_plus && token.is("BEGINEXT") {
                self.extension(&token)?;
                after_plus = false;
            } else if after_plus && token.is("PROPERTY") {
                loop {
                    let name = self
                        .next_context(false, false)?
                        .ok_or_else(|| token.error("unterminated PROPERTY"))?;
                    if name.is(";") {
                        return Ok(());
                    }
                    if name.is("+") {
                        after_plus = true;
                        break;
                    }
                    self.next_context(true, false)?
                        .ok_or_else(|| token.error("missing PROPERTY value"))?;
                }
            } else {
                string_value = (property_definition && token.is("STRING"))
                    || (after_plus && token.is("NETEXPR"));
                after_plus = token.is("+");
            }
        }
    }
}

struct Clause {
    keyword: Token,
    values: Vec<Token>,
}

struct Pin {
    name: Token,
    clauses: Vec<Clause>,
}

struct Design {
    name: Token,
    units: Option<Token>,
    bus_chars: (char, char),
    die_area: Option<(Token, Vec<Token>)>,
    pins: Vec<Pin>,
}

fn parse(def: &str) -> Result<Design> {
    let mut lexer = Lexer::new(def);
    let mut name = None;
    let mut units = None;
    let mut bus_chars = ('[', ']');
    let mut die_area = None;
    let mut pins = Vec::new();
    let mut has_pins = false;
    let mut ended = false;
    while let Some(token) = lexer.next()? {
        match token.text.to_ascii_uppercase().as_str() {
            "DESIGN" => {
                if name.is_some() {
                    return Err(token.error("duplicate DESIGN"));
                }
                name = Some(lexer.name()?);
                lexer.expect(";")?;
            }
            "UNITS" => {
                if units.is_some() {
                    return Err(token.error("duplicate UNITS"));
                }
                lexer.expect("DISTANCE")?;
                lexer.expect("MICRONS")?;
                let value = lexer.required()?;
                if value.integer()? <= 0 {
                    return Err(value.error("units must be positive"));
                }
                units = Some(value);
                lexer.expect(";")?;
            }
            "BUSBITCHARS" => {
                let value = lexer.required()?;
                let chars: Vec<_> = value.text.chars().collect();
                if chars.len() != 2 || chars[0] == chars[1] {
                    return Err(value.error("BUSBITCHARS requires two different characters"));
                }
                bus_chars = (chars[0], chars[1]);
                lexer.expect(";")?;
            }
            "DIEAREA" => {
                if die_area.is_some() {
                    return Err(token.error("duplicate DIEAREA"));
                }
                die_area = Some((token, lexer.statement()?));
            }
            "PINS" => {
                if has_pins {
                    return Err(token.error("duplicate PINS section"));
                }
                has_pins = true;
                let count = lexer.required()?;
                let expected = usize::try_from(count.integer()?)
                    .map_err(|_| count.error("negative pin count"))?;
                lexer.expect(";")?;
                loop {
                    let start = lexer.required()?;
                    if start.is("END") {
                        lexer.expect("PINS")?;
                        break;
                    }
                    if !start.is("-") {
                        return Err(start.error("expected a pin record or END PINS"));
                    }
                    let pin_name = lexer.name()?;
                    lexer.expect("+")?;
                    lexer.expect("NET")?;
                    lexer.name()?;
                    let mut clauses = Vec::new();
                    let mut delimiter = lexer.required()?;
                    while !delimiter.is(";") {
                        if !delimiter.is("+") {
                            return Err(delimiter.error("expected '+' or ';' in pin"));
                        }
                        let keyword = lexer.required()?;
                        if keyword.is("BEGINEXT") {
                            lexer.extension(&keyword)?;
                            delimiter = lexer.required()?;
                            continue;
                        }
                        let mut values = Vec::new();
                        if [
                            "LAYER",
                            "POLYGON",
                            "VIA",
                            "DIRECTION",
                            "USE",
                            "SUPPLYSENSITIVITY",
                            "GROUNDSENSITIVITY",
                        ]
                        .iter()
                        .any(|k| keyword.is(k))
                        {
                            values.push(lexer.name()?);
                        }
                        let coordinate_clause =
                            ["LAYER", "POLYGON", "VIA", "PLACED", "FIXED", "COVER"]
                                .iter()
                                .any(|k| keyword.is(k));
                        let mut property_name = true;
                        loop {
                            delimiter = if keyword.is("PROPERTY") {
                                let value = lexer
                                    .next_context(!property_name, false)?
                                    .ok_or_else(|| keyword.error("unterminated PROPERTY"))?;
                                property_name = !property_name;
                                value
                            } else if coordinate_clause {
                                lexer.required()?
                            } else {
                                // Antenna and other ignored clauses may contain
                                // identifiers beginning with a literal quote.
                                // NETEXPR is the string-valued exception.
                                lexer
                                    .next_context(keyword.is("NETEXPR"), false)?
                                    .ok_or_else(|| keyword.error("unterminated pin clause"))?
                            };
                            if delimiter.is("+") || delimiter.is(";") {
                                break;
                            }
                            values.push(delimiter);
                        }
                        clauses.push(Clause { keyword, values });
                    }
                    pins.push(Pin {
                        name: pin_name,
                        clauses,
                    });
                }
                if pins.len() != expected {
                    return Err(count.error(format!(
                        "PINS declares {expected} records but contains {}",
                        pins.len()
                    )));
                }
            }
            "BEGINEXT" => lexer.extension(&token)?,
            "HISTORY" => loop {
                match lexer.bump() {
                    Some(';') => break,
                    Some(_) => {}
                    None => return Err(token.error("unterminated HISTORY")),
                }
            },
            "PROPERTYDEFINITIONS" => lexer.skip_section(&token)?,
            "COMPONENTS" | "VIAS" | "REGIONS" | "BLOCKAGES" | "SLOTS" | "FILLS" | "NETS"
            | "SPECIALNETS" | "SCANCHAINS" | "GROUPS" | "NONDEFAULTRULES" | "PINPROPERTIES"
            | "STYLES" | "ASSERTIONS" | "CONSTRAINTS" | "IOTIMINGS" | "TIMINGDISABLES"
            | "PARTITIONS" | "FPC" | "DEFAULTCAP" => {
                lexer.skip_statement()?;
                lexer.skip_section(&token)?;
            }
            "END" => {
                lexer.expect("DESIGN")?;
                ended = true;
                break;
            }
            _ => lexer.skip_record(false)?,
        }
    }
    if !ended {
        return Err("DEF: missing END DESIGN".to_string());
    }
    if let Some(token) = lexer.next()? {
        return Err(token.error("unexpected content after END DESIGN"));
    }
    let name = name.ok_or("DEF: missing DESIGN")?;
    Ok(Design {
        name,
        units,
        bus_chars,
        die_area,
        pins,
    })
}

fn decoded_name(token: &Token) -> Result<Vec<(char, bool)>> {
    let mut chars = token.text.chars();
    let mut result = Vec::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push((
                chars
                    .next()
                    .ok_or_else(|| token.error("unterminated identifier escape"))?,
                true,
            ));
        } else {
            result.push((ch, false));
        }
    }
    Ok(result)
}

fn pin_name(token: &Token, bus_chars: (char, char)) -> Result<(String, usize, bool)> {
    let chars = decoded_name(token)?;
    if chars.last() == Some(&(bus_chars.1, false))
        && let Some(open) = chars.iter().rposition(|ch| *ch == (bus_chars.0, false))
    {
        let digits: String = chars[open + 1..chars.len() - 1]
            .iter()
            .map(|c| c.0)
            .collect();
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let bit = digits
                .parse::<usize>()
                .map_err(|_| token.error("bus index is too large"))?;
            let base: String = chars[..open].iter().map(|c| c.0).collect();
            if base.is_empty() {
                return Err(token.error("empty bus name"));
            }
            return Ok((base, bit, true));
        }
    }
    Ok((chars.iter().map(|c| c.0).collect(), 0, false))
}

fn points(tokens: &[Token], source: &Token) -> Result<Vec<(i64, i64)>> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if cursor + 3 >= tokens.len() || !tokens[cursor].is("(") || !tokens[cursor + 3].is(")") {
            return Err(tokens[cursor].error("expected a coordinate pair '( x y )'"));
        }
        let previous = result.last().copied();
        let x = &tokens[cursor + 1];
        let y = &tokens[cursor + 2];
        let x = if x.is("*") {
            previous
                .map(|p: (i64, i64)| p.0)
                .ok_or_else(|| x.error("'*' has no previous coordinate"))?
        } else {
            x.integer()?
        };
        let y = if y.is("*") {
            previous
                .map(|p| p.1)
                .ok_or_else(|| y.error("'*' has no previous coordinate"))?
        } else {
            y.integer()?
        };
        result.push((x, y));
        cursor += 4;
    }
    if result.is_empty() {
        return Err(source.error("missing coordinates"));
    }
    Ok(result)
}

fn scale(value: i128, units: i64, target: i64, source: &Token) -> Result<i64> {
    let numerator = value
        .checked_mul(target as i128)
        .ok_or_else(|| source.error("coordinate overflow"))?;
    if numerator % units as i128 != 0 {
        return Err(source.error("coordinate is not exactly representable in target units_microns"));
    }
    i64::try_from(numerator / units as i128).map_err(|_| source.error("coordinate overflow"))
}

fn unit_value(units: Option<&Token>, source: &Token) -> Result<i64> {
    units
        .ok_or_else(|| source.error("geometry requires UNITS DISTANCE MICRONS"))?
        .integer()
}

fn rectangle(a: (i64, i64), b: (i64, i64), source: &Token) -> Result<Vec<(i64, i64)>> {
    let (x1, x2) = (a.0.min(b.0), a.0.max(b.0));
    let (y1, y2) = (a.1.min(b.1), a.1.max(b.1));
    if x1 == x2 || y1 == y2 {
        return Err(source.error("rectangle has zero area"));
    }
    Ok(vec![(x1, y1), (x1, y2), (x2, y2), (x2, y1)])
}

/// Convert DEF DIEAREA coordinates into the outline expected by ModDef.
/// Expand a rectangle's two corners into four vertices, remove duplicate points
/// and extra points along straight edges, and convert to the target unit scale.
/// Order the vertices clockwise, starting with the leftmost vertical edge.
fn canonical_die(tokens: &[Token], source: &Token, units: i64, target: i64) -> Result<Polygon> {
    let mut raw = points(tokens, source)?;
    if raw.len() == 2 {
        raw = rectangle(raw[0], raw[1], source)?;
    }
    raw.dedup();
    if raw.first() == raw.last() {
        raw.pop();
    }
    // Remove extra points along straight edges so a rectangle has four edges,
    // whether it was specified as two corners or as a polygon.
    while let Some(index) = (0..raw.len()).find(|&i| {
        if raw.len() < 3 {
            return false;
        }
        let a = raw[(i + raw.len() - 1) % raw.len()];
        let b = raw[i];
        let c = raw[(i + 1) % raw.len()];
        (a.0 == b.0 && b.0 == c.0 && b.1 >= a.1.min(c.1) && b.1 <= a.1.max(c.1))
            || (a.1 == b.1 && b.1 == c.1 && b.0 >= a.0.min(c.0) && b.0 <= a.0.max(c.0))
    }) {
        raw.remove(index);
    }
    if raw.len() < 4 {
        return Err(source.error("DIEAREA needs a non-degenerate rectilinear polygon"));
    }
    let vertices = raw
        .iter()
        .map(|&(x, y)| {
            Ok(Coordinate {
                x: scale(x as i128, units, target, source)?,
                y: scale(y as i128, units, target, source)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut polygon = Polygon::new(vertices);
    if !polygon.is_rectilinear() {
        return Err(source.error("DIEAREA must be rectilinear"));
    }
    let mut area = 0i128;
    for i in 0..polygon.0.len() {
        let a = polygon.0[i];
        let b = polygon.0[(i + 1) % polygon.0.len()];
        let cross = (a.x as i128 * b.y as i128)
            .checked_sub(b.x as i128 * a.y as i128)
            .ok_or_else(|| source.error("DIEAREA arithmetic overflow"))?;
        area = area
            .checked_add(cross)
            .ok_or_else(|| source.error("DIEAREA arithmetic overflow"))?;
    }
    if area == 0 {
        return Err(source.error("DIEAREA has zero area"));
    }
    if area > 0 {
        polygon.0.reverse();
    }
    let first = (0..polygon.0.len())
        .filter(|&i| polygon.0[i].x == polygon.0[(i + 1) % polygon.0.len()].x)
        .min_by_key(|&i| {
            (
                polygon.0[i].x,
                polygon.0[i].y.min(polygon.0[(i + 1) % polygon.0.len()].y),
            )
        })
        .unwrap();
    polygon.0.rotate_left(first);
    Ok(polygon)
}

fn validate_pin_polygon(points: &[(i64, i64)], source: &Token) -> Result<()> {
    let unique: HashSet<_> = points.iter().collect();
    if unique.len() != points.len() {
        return Err(source.error("POLYGON has repeated vertices"));
    }
    let mut area = 0i128;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let dx = b.0 as i128 - a.0 as i128;
        let dy = b.1 as i128 - a.1 as i128;
        if dx != 0 && dy != 0 && dx.abs() != dy.abs() {
            return Err(source.error("POLYGON edges must be orthogonal or at 45 degrees"));
        }
        let cross = (a.0 as i128 * b.1 as i128)
            .checked_sub(b.0 as i128 * a.1 as i128)
            .ok_or_else(|| source.error("POLYGON arithmetic overflow"))?;
        area = area
            .checked_add(cross)
            .ok_or_else(|| source.error("POLYGON arithmetic overflow"))?;
    }
    if area == 0 {
        return Err(source.error("POLYGON has zero area"));
    }
    Ok(())
}

fn geometry(
    pin: &Pin,
    opts: &LefDefOptions,
    units: Option<&Token>,
    selected_layer: Option<&str>,
) -> Result<Option<PhysicalPin>> {
    let groups = pin.clauses.split(|clause| clause.keyword.is("PORT"));
    let mut geometries = Vec::new();
    'groups: for group in groups {
        let placements: Vec<_> = group
            .iter()
            .filter(|c| ["PLACED", "FIXED", "COVER"].iter().any(|k| c.keyword.is(k)))
            .collect();
        if placements.is_empty() {
            continue;
        }
        for clause in group {
            let via = clause.keyword.is("VIA");
            if !(via || clause.keyword.is("LAYER") || clause.keyword.is("POLYGON")) {
                continue;
            }
            if via {
                geometries.push(ImportedPinGeometry::Unsupported("VIA".to_string()));
                continue;
            }
            let layer_token = clause
                .values
                .first()
                .ok_or_else(|| clause.keyword.error("missing geometry layer"))?;
            let layer: String = decoded_name(layer_token)?.iter().map(|c| c.0).collect();
            if opts
                .valid_pin_layers
                .as_ref()
                .is_some_and(|layers| !layers.contains(&layer))
                || selected_layer.is_some_and(|selected| selected != layer)
            {
                continue;
            }
            if placements.len() != 1 {
                return Err(pin.name.error("multiple placements for one pin PORT"));
            }
            let placement = placements[0];
            if placement.values.len() != 5 {
                return Err(placement
                    .keyword
                    .error("expected placement '( x y ) orientation'"));
            }
            let origin = points(&placement.values[..4], &placement.keyword)?[0];
            let orientation = &placement.values[4];
            let start = clause
                .values
                .iter()
                .position(|t| t.is("("))
                .ok_or_else(|| clause.keyword.error("missing geometry coordinates"))?;
            // MASK, SPACING and DESIGNRULEWIDTH are deliberately not retained.
            let mut coords = points(&clause.values[start..], &clause.keyword)?;
            if clause.keyword.is("LAYER") {
                if coords.len() != 2 {
                    return Err(clause.keyword.error("LAYER needs two rectangle corners"));
                }
                coords = rectangle(coords[0], coords[1], &clause.keyword)?;
            } else {
                if coords.first() == coords.last() {
                    coords.pop();
                }
                if coords.len() < 3 {
                    return Err(clause
                        .keyword
                        .error("POLYGON requires at least three vertices"));
                }
                validate_pin_polygon(&coords, &clause.keyword)?;
            }
            let units = unit_value(units, &clause.keyword)?;
            let vertices = coords
                .iter()
                .map(|&(x, y)| {
                    let (x, y) = (x as i128, y as i128);
                    let (x, y) = match orientation.text.to_ascii_uppercase().as_str() {
                        "N" => (x, y),
                        "S" => (-x, -y),
                        "W" => (-y, x),
                        "E" => (y, -x),
                        "FN" => (-x, y),
                        "FS" => (x, -y),
                        "FW" => (y, x),
                        "FE" => (-y, -x),
                        _ => return Err(orientation.error("unknown pin orientation")),
                    };
                    Ok(Coordinate {
                        x: scale(
                            x + origin.0 as i128,
                            units,
                            opts.units_microns,
                            &clause.keyword,
                        )?,
                        y: scale(
                            y + origin.1 as i128,
                            units,
                            opts.units_microns,
                            &clause.keyword,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            geometries.push(ImportedPinGeometry::Polygon {
                layer,
                polygon: Polygon::new(vertices),
            });
            if opts.multiple_pin_shapes_policy == MultiplePinShapesPolicy::First {
                break 'groups;
            }
        }
    }
    resolve_pin_geometry(
        geometries.into_iter().map(Ok),
        opts.multiple_pin_shapes_policy,
        selected_layer,
    )
    .map_err(|message| pin.name.error(message))
}

struct Port {
    source: Token,
    is_bus: bool,
    direction: String,
    pin_use: String,
    bits: IndexMap<usize, Option<PhysicalPin>>,
}

fn construct(def: &str, opts: &LefDefOptions) -> Result<ModDef> {
    if opts.units_microns <= 0 {
        return Err("DEF: target units_microns must be positive".to_string());
    }
    let design = parse(def)?;
    let shape = design
        .die_area
        .as_ref()
        .map(|(source, values)| {
            canonical_die(
                values,
                source,
                unit_value(design.units.as_ref(), source)?,
                opts.units_microns,
            )
        })
        .transpose()?;
    let mut ports: IndexMap<String, Port> = IndexMap::new();
    for pin in design.pins {
        let (name, bit, is_bus) = pin_name(&pin.name, design.bus_chars)?;
        if opts.ignore_pin_names.contains(&name) {
            continue;
        }
        let pin_use = pin
            .clauses
            .iter()
            .rev()
            .find(|c| c.keyword.is("USE"))
            .and_then(|c| c.values.first())
            .map(|v| v.text.to_ascii_uppercase())
            .unwrap_or_else(|| "SIGNAL".to_string());
        if opts
            .skip_pin_uses
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&pin_use))
        {
            continue;
        }
        let direction = pin
            .clauses
            .iter()
            .rev()
            .find(|c| c.keyword.is("DIRECTION"))
            .and_then(|c| c.values.first())
            .map(|v| v.text.to_ascii_uppercase())
            .unwrap_or_else(|| "INOUT".to_string());
        let direction = match direction.as_str() {
            "INPUT" | "OUTPUT" | "INOUT" => direction,
            "FEEDTHRU" => "INOUT".to_string(),
            _ => return Err(pin.name.error(format!("unsupported DIRECTION {direction}"))),
        };
        let selected_layer = opts
            .pin_layer_selections
            .get(&(name.clone(), bit))
            .map(String::as_str);
        let physical_pin = geometry(&pin, opts, design.units.as_ref(), selected_layer)?;
        let port = ports.entry(name.clone()).or_insert_with(|| Port {
            source: pin.name.clone(),
            is_bus,
            direction: direction.clone(),
            pin_use: pin_use.clone(),
            bits: IndexMap::new(),
        });
        if port.is_bus != is_bus {
            return Err(pin
                .name
                .error(format!("ambiguous scalar/bus name '{name}'")));
        }
        if port.direction != direction {
            return Err(pin
                .name
                .error(format!("mismatched directions for bus '{name}'")));
        }
        if port.pin_use != pin_use {
            return Err(pin
                .name
                .error(format!("mismatched USE values for bus '{name}'")));
        }
        if port.bits.insert(bit, physical_pin).is_some() {
            return Err(pin.name.error(format!("duplicate pin '{name}' bit {bit}")));
        }
    }
    // Validate widths without iterating up to a potentially enormous input index.
    for (name, port) in &ports {
        let max = port.bits.keys().max().copied().unwrap();
        if max.checked_add(1) != Some(port.bits.len()) {
            return Err(port.source.error(format!(
                "pin '{name}' has non-contiguous or nonzero-based bit indices"
            )));
        }
    }
    let name: String = decoded_name(&design.name)?.iter().map(|c| c.0).collect();
    let result = ModDef::new(name);
    if let Some(shape) = shape {
        result.set_shape(shape);
    }
    for (name, port) in ports {
        let width = port.bits.len();
        let io = match port.direction.as_str() {
            "INPUT" => IO::Input(width),
            "OUTPUT" => IO::Output(width),
            _ => IO::InOut(width),
        };
        result.add_port(&name, io);
        for (bit, physical_pin) in port.bits {
            if let Some(physical_pin) = physical_pin {
                result.place_pin(&name, bit, physical_pin);
            }
        }
    }
    Ok(result)
}

pub(crate) fn mod_def_from_def(def: &str, opts: &LefDefOptions) -> ModDef {
    construct(def, opts).unwrap_or_else(|message| panic!("{message}"))
}
