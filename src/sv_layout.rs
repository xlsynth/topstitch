// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::packed::{Shape, extent};
use crate::{DataType, EnumValue, PackedLayout};
use slang_rs::Type;

impl DataType {
    /// Takes ownership of a resolved SystemVerilog type and retains its wiring
    /// layout and enum encodings. A live package lookup can supply the type
    /// with `package.ty(name)?.into_owned()`.
    pub fn from_sv_type(ty: Type) -> Self {
        let width = ty.width().ok();
        Self {
            width,
            shape: wiring_shape(ty, width),
        }
    }
}

impl PackedLayout {
    /// Imports the layout of a resolved SystemVerilog packed struct, array, or enum.
    /// Select struct members with [`Self::field`] and array indices with [`Self::element`].
    /// Returns an error for unpacked types and scalars without a selectable layout.
    /// Each import creates an owned root. Reuse that root or its clones when comparing selections with [`Self::range_in`].
    ///
    /// ```
    /// use topstitch::{IO, ModDef, PackageLookup, PackedLayout, ParserConfig, Source, VerilogLibrary};
    /// let sources = [Source::text(
    ///     "package display; typedef struct packed { logic [7:0] r, g, b; } rgb; endpackage",
    /// )];
    /// let lib = VerilogLibrary::from_config(&ParserConfig { sources: &sources, ..Default::default() })?;
    /// let display = lib.package("display")?;
    /// let ty = display.ty("rgb")?;
    /// let rgb = PackedLayout::from_sv_type(&ty)?;
    /// drop(ty);
    /// drop(lib);
    /// let green_bits = rgb.field("g").range_in_root();
    /// assert_eq!((green_bits.msb(), green_bits.lsb()), (15, 8));
    /// let module = ModDef::new("screen");
    /// let pixels = module.add_port("pixels", IO::Input(32));
    /// let green = rgb.on(&pixels.slice(27, 4)).field("g");
    /// drop(rgb);
    /// assert_eq!(green.bit(0), pixels.bit(12));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_sv_type(ty: &Type) -> Result<Self, &'static str> {
        let name = match ty {
            Type::Struct { name, .. } | Type::Enum { name, .. } | Type::Union { name, .. } => {
                name.as_deref()
            }
            Type::Integral { .. } => None,
        };
        DataType::from_sv_type(ty.clone())
            .layout_with_label(name.unwrap_or("<anonymous>"))
            .ok_or("expected a SystemVerilog packed struct, array, or enum")
    }
}

/// Builds the packed shape, retaining element enumerators even for unpacked enum arrays.
fn wiring_shape(ty: Type, width: Option<usize>) -> Option<Arc<Shape>> {
    let (mut shape, dims) = match ty {
        Type::Enum {
            variants,
            packed_dimensions,
            base_type,
            ..
        } => {
            let members = variants
                .into_iter()
                .map(|v| (v.name, EnumValue { integer: v.value }))
                .collect();
            let shape = Shape::enumeration(base_type.width().expect("enum base width"), members);
            if width.is_none() {
                return Some(shape);
            }
            (shape, packed_dimensions)
        }
        Type::Struct {
            is_packed: true,
            fields,
            packed_dimensions,
            ..
        } if width.is_some() => {
            let shape = Shape::structure(fields.into_iter().map(|field| {
                let member = DataType::from_sv_type(field.ty);
                let shape = member
                    .shape
                    .unwrap_or_else(|| Shape::bits(member.width.expect("packed field width")));
                (Arc::from(field.name), shape)
            }));
            (shape, packed_dimensions)
        }
        Type::Struct {
            packed_dimensions, ..
        }
        | Type::Integral {
            packed_dimensions, ..
        }
        | Type::Union {
            packed_dimensions, ..
        } => {
            let width = width?;
            if packed_dimensions.is_empty() {
                return None;
            }
            let base_width = packed_dimensions
                .iter()
                .fold(width, |w, d| w / extent(d.left, d.right));
            (Shape::bits(base_width), packed_dimensions)
        }
    };
    for dim in dims.iter().rev() {
        shape = shape.array(dim.left, dim.right);
    }
    Some(shape)
}
