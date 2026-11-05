// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::DataType;
use crate::packed::{Shape, extent};
use slang_rs::Type;

/// Extracts selectable wiring geometry from an elaborated type. Scalars and
/// unpacked types have no packed selection to retain.
pub(crate) fn packed_shape(ty: &Type) -> Option<Arc<Shape>> {
    let width = ty.width().ok()?;
    let dims = match ty {
        Type::Struct {
            packed_dimensions, ..
        }
        | Type::Integral {
            packed_dimensions, ..
        }
        | Type::Enum {
            packed_dimensions, ..
        }
        | Type::Union {
            packed_dimensions, ..
        } => packed_dimensions,
    };
    let mut shape = match ty {
        Type::Struct {
            fields,
            is_packed: true,
            ..
        } => Shape::structure(fields.iter().map(|field| {
            let member = packed_shape(&field.ty)
                .unwrap_or_else(|| Shape::bits(field.ty.width().expect("packed field width")));
            (Arc::from(field.name.as_str()), member)
        })),
        _ if dims.is_empty() => return None,
        _ => {
            let base_width = dims.iter().fold(width, |w, d| w / extent(d.left, d.right));
            Shape::bits(base_width)
        }
    };
    for dim in dims.iter().rev() {
        shape = shape.array(dim.left, dim.right);
    }
    Some(shape)
}

/// Extracts wiring geometry and enum members from an elaborated declaration.
pub(crate) fn data_type(ty: Type) -> DataType {
    let width = ty.width().ok();
    let shape = packed_shape(&ty);
    let enum_members = match ty {
        Type::Enum { variants, .. } => {
            Some(variants.into_iter().map(|v| (v.name, v.value)).collect())
        }
        _ => None,
    };
    DataType {
        width,
        shape,
        enum_members,
    }
}
