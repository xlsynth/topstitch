// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::packed::{Shape, extent};
use crate::{DataType, EnumValue};
use slang_rs::Type;

/// Extracts packed wiring geometry and enum encodings from an elaborated declaration.
pub(crate) fn data_type(ty: Type) -> DataType {
    let width = ty.width().ok();
    DataType {
        width,
        shape: wiring_shape(ty, width),
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
                let member = data_type(field.ty);
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
