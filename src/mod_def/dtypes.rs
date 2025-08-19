// SPDX-License-Identifier: Apache-2.0

use crate::{PipelineConfig, PortSlice};

pub(crate) struct VerilogImport {
    pub(crate) sources: Vec<String>,
    pub(crate) incdirs: Vec<String>,
    pub(crate) defines: Vec<(String, String)>,
    pub(crate) skip_unsupported: bool,
    pub(crate) ignore_unknown_modules: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Assignment {
    pub lhs: PortSlice,
    pub rhs: PortSlice,
    pub pipeline: Option<PipelineConfig>,
    pub is_non_abutted: bool,
}

#[derive(Clone)]
pub(crate) struct InstConnection {
    pub(crate) inst_port_slice: PortSlice,
    pub(crate) connected_to: PortSliceOrWire,
}

#[derive(Clone)]
pub(crate) struct Wire {
    pub(crate) name: String,
    pub(crate) width: usize,
}

#[derive(Clone)]
pub(crate) enum PortSliceOrWire {
    PortSlice(PortSlice),
    Wire(Wire),
}

// Floorplanning-related types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coordinate {
    pub x: i64,
    pub y: i64,
}

impl From<(i64, i64)> for Coordinate {
    fn from(value: (i64, i64)) -> Self {
        Coordinate {
            x: value.0,
            y: value.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    R0,
    R90,
    R180,
    R270,
    MX,
    MY,
    MX90,
    MY90,
}

#[derive(Debug, Clone)]
pub struct RectilinearShape(pub Vec<Coordinate>);

pub struct BoundingBox {
    pub min_x: i64,
    pub max_x: i64,
    pub min_y: i64,
    pub max_y: i64,
}

impl RectilinearShape {
    pub fn new(points: Vec<Coordinate>) -> Self {
        assert!(
            is_rectilinear(&points),
            "Only rectilinear polygons are supported"
        );
        RectilinearShape(points)
    }

    pub fn from_width_height(width: i64, height: i64) -> Self {
        Self::new(vec![
            Coordinate { x: 0, y: 0 },
            Coordinate { x: width, y: 0 },
            Coordinate {
                x: width,
                y: height,
            },
            Coordinate { x: 0, y: height },
        ])
    }

    pub fn apply_transform(&self, m: &Mat3) -> RectilinearShape {
        let pts = self
            .0
            .iter()
            .map(|p| {
                let v = nalgebra::Vector3::new(p.x, p.y, 1);
                let result = m.0 * v;
                Coordinate {
                    x: result[0],
                    y: result[1],
                }
            })
            .collect();
        RectilinearShape(pts)
    }

    pub fn bbox(&self) -> BoundingBox {
        let mut min_x = i64::MAX;
        let mut max_x = i64::MIN;
        let mut min_y = i64::MAX;
        let mut max_y = i64::MIN;
        for p in &self.0 {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        BoundingBox {
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }
}

impl PartialEq for RectilinearShape {
    fn eq(&self, other: &Self) -> bool {
        let a = &self.0;
        let b = &other.0;
        if a.len() != b.len() {
            return false;
        }
        if a.is_empty() {
            return true;
        }
        // rotation-invariant equality: try aligning every index
        for start in 0..b.len() {
            let mut all_match = true;
            for (i, a_i) in a.iter().enumerate() {
                let j = (start + i) % b.len();
                if a_i != &b[j] {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                return true;
            }
        }
        false
    }
}

impl Eq for RectilinearShape {}

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub coordinate: Coordinate,
    pub orientation: Orientation,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            coordinate: Coordinate { x: 0, y: 0 },
            orientation: Orientation::R0,
        }
    }
}

pub(crate) fn is_rectilinear(points: &[Coordinate]) -> bool {
    // Check that all edges are axis-aligned
    for i in 0..points.len() {
        let a = points[i];
        // Modulo is needed to check the last edge
        let b = points[(i + 1) % points.len()];
        if !(a.x == b.x || a.y == b.y) {
            return false;
        }
        if a.x == b.x && a.y == b.y {
            // degenerate edge
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Mat3(pub nalgebra::Matrix3<i64>);

impl Mat3 {
    pub fn identity() -> Mat3 {
        Mat3(nalgebra::Matrix3::identity())
    }

    pub fn translate(dx: i64, dy: i64) -> Mat3 {
        Mat3(nalgebra::Matrix3::new(1, 0, dx, 0, 1, dy, 0, 0, 1))
    }

    pub fn from_orientation(o: Orientation) -> Mat3 {
        const ROTATE_90: Mat3 = Mat3(nalgebra::Matrix3::new(0, -1, 0, 1, 0, 0, 0, 0, 1));
        const MIRROR_X: Mat3 = Mat3(nalgebra::Matrix3::new(1, 0, 0, 0, -1, 0, 0, 0, 1));
        const MIRROR_Y: Mat3 = Mat3(nalgebra::Matrix3::new(-1, 0, 0, 0, 1, 0, 0, 0, 1));

        match o {
            Orientation::R0 => Self::identity(),
            Orientation::R90 => ROTATE_90,
            Orientation::R180 => &ROTATE_90 * &ROTATE_90,
            Orientation::R270 => &(&ROTATE_90 * &ROTATE_90) * &ROTATE_90,
            Orientation::MX => MIRROR_X,
            Orientation::MY => MIRROR_Y,
            Orientation::MX90 => &ROTATE_90 * &MIRROR_X,
            Orientation::MY90 => &ROTATE_90 * &MIRROR_Y,
        }
    }

    pub fn as_orientation(&self) -> Orientation {
        for orientation in [
            Orientation::R0,
            Orientation::R90,
            Orientation::R180,
            Orientation::R270,
            Orientation::MX,
            Orientation::MY,
            Orientation::MX90,
            Orientation::MY90,
        ] {
            let ref_mat = Self::from_orientation(orientation);
            if self.0.fixed_view::<2, 2>(0, 0) == ref_mat.0.fixed_view::<2, 2>(0, 0) {
                return orientation;
            }
        }
        panic!("Unsupported orientation: {:?}", self);
    }
}

impl std::ops::Mul<&Mat3> for &Mat3 {
    type Output = Mat3;
    fn mul(self, rhs: &Mat3) -> Mat3 {
        Mat3(self.0 * rhs.0)
    }
}
