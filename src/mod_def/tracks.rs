// SPDX-License-Identifier: Apache-2.0

use crate::mod_def::dtypes::{EdgeOrientation, Polygon, Range};
use indexmap::IndexMap;

use fixedbitset::FixedBitSet;

use crate::mod_def::{
    BOTTOM_EDGE_INDEX, EAST_EDGE_INDEX, LEFT_EDGE_INDEX, NORTH_EDGE_INDEX, RIGHT_EDGE_INDEX,
    SOUTH_EDGE_INDEX, TOP_EDGE_INDEX, WEST_EDGE_INDEX,
};
use crate::{Coordinate, Mat3, ModDef, Orientation};

use std::fmt;

/// Error type describing why a pin or keepout cannot be placed on a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinPlacementError {
    /// The module shape or track occupancies were not initialized.
    NotInitialized(&'static str),
    /// The requested edge index is out of bounds for the current shape.
    EdgeOutOfBounds { edge_index: usize, num_edges: usize },
    /// The requested routing layer is not present on this edge.
    LayerUnavailable { layer: String },
    /// The requested pin span is out of bounds for the available tracks on this
    /// edge.
    OutOfBounds {
        min_index: i64,
        max_index: i64,
        num_tracks: usize,
    },
    /// The requested pin overlaps an existing pin.
    OverlapsExistingPin { min_index: i64, max_index: i64 },
    /// The requested pin overlaps a keepout region.
    OverlapsKeepout { min_index: i64, max_index: i64 },
}

impl fmt::Display for PinPlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinPlacementError::NotInitialized(what) => {
                write!(
                    f,
                    "{what} not initialized; call set_shape and set_track_definitions first"
                )
            }
            PinPlacementError::EdgeOutOfBounds {
                edge_index,
                num_edges,
            } => write!(
                f,
                "edge index {edge_index} is out of bounds ({num_edges} edges available)"
            ),
            PinPlacementError::LayerUnavailable { layer } => {
                write!(f, "layer '{layer}' has no tracks on this edge")
            }
            PinPlacementError::OutOfBounds {
                min_index,
                max_index,
                num_tracks,
            } => write!(
                f,
                "requested track span [{min_index}..={max_index}] is outside available range [0..={}]",
                num_tracks.saturating_sub(1)
            ),
            PinPlacementError::OverlapsExistingPin {
                min_index,
                max_index,
            } => write!(
                f,
                "requested track span [{min_index}..={max_index}] overlaps an existing pin"
            ),
            PinPlacementError::OverlapsKeepout {
                min_index,
                max_index,
            } => write!(
                f,
                "requested track span [{min_index}..={max_index}] overlaps a keepout region"
            ),
        }
    }
}

impl std::error::Error for PinPlacementError {}

/// Orientation of routing tracks.
#[derive(Clone, PartialEq, Eq)]
pub enum TrackOrientation {
    Horizontal,
    Vertical,
}

impl TrackOrientation {
    /// Returns `true` when this track orientation can legally place pins on an
    /// edge with the supplied [`EdgeOrientation`]. Horizontal tracks may only
    /// service north/south edges, while vertical tracks may only service
    /// east/west edges.
    pub fn is_compatible_with_edge_orientation(&self, edge_orientation: &EdgeOrientation) -> bool {
        matches!(
            (self, edge_orientation),
            (
                TrackOrientation::Horizontal,
                EdgeOrientation::North | EdgeOrientation::South
            ) | (
                TrackOrientation::Vertical,
                EdgeOrientation::East | EdgeOrientation::West
            )
        )
    }
}

/// Definition of a routing track family on a named layer.
#[derive(Clone)]
pub struct TrackDefinition {
    pub(crate) name: String,
    pub(crate) offset: i64,
    pub(crate) period: i64,
    pub(crate) orientation: TrackOrientation,
    pub(crate) pin_shape: Option<Polygon>,
    pub(crate) keepout_shape: Option<Polygon>,
}

impl TrackDefinition {
    /// Create a new [`TrackDefinition`].
    /// - `name`: routing layer name (e.g. "M1")
    /// - `offset`: first track coordinate relative to the edge origin
    /// - `period`: spacing between adjacent tracks
    /// - `orientation`: horizontal or vertical
    /// - `pin_shape`: optional pin polygon relative to the track origin
    /// - `keepout_shape`: optional keepout polygon relative to the track origin
    pub fn new(
        name: impl AsRef<str>,
        offset: i64,
        period: i64,
        orientation: TrackOrientation,
        pin_shape: Option<Polygon>,
        keepout_shape: Option<Polygon>,
    ) -> Self {
        TrackDefinition {
            name: name.as_ref().to_string(),
            offset,
            period,
            orientation,
            pin_shape,
            keepout_shape,
        }
    }

    /// Convert a coordinate range to track indices such that converting the
    /// quantized range back to coordinates will not exceed the original
    /// range.
    pub fn convert_coord_range_to_index_range(&self, range: &Range) -> Range {
        debug_assert!(self.period > 0);

        Range {
            min: range
                .min
                .map(|min| (min - self.offset + self.period - 1) / self.period),
            max: range.max.map(|max| (max - self.offset) / self.period),
        }
    }

    /// Convert a track index range to a coordinate range.
    pub fn convert_index_range_to_coord_range(&self, range: &Range) -> Range {
        Range {
            min: range.min.map(|min| self.offset + (min * self.period)),
            max: range.max.map(|max| self.offset + (max * self.period)),
        }
    }

    /// Convert a track index to a coordinate.
    pub fn index_to_position(&self, index: i64) -> i64 {
        self.offset + (index * self.period)
    }

    /// Returns the nearest track index (in track coordinates, before
    /// edge-relative normalization) to the provided coordinate.
    pub fn nearest_track_index(&self, coordinate: i64) -> i64 {
        assert!(self.period != 0, "Track period must be non-zero");
        let n = coordinate - self.offset;
        let p = self.period;
        let mut q = n / p;
        let r = n % p;

        if 2 * r.abs() >= p.abs() {
            q += if n >= 0 { 1 } else { -1 };
        }

        q
    }
}

/// Collection of track definitions keyed by layer name.
#[derive(Clone)]
pub struct TrackDefinitions(pub(crate) IndexMap<String, TrackDefinition>);

impl TrackDefinitions {
    /// Creates an empty collection of track definitions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a track definition.
    pub fn add_track(&mut self, track: TrackDefinition) {
        self.0.insert(track.name.clone(), track);
    }

    /// Get a track definition by layer name.
    pub fn get_track(&self, name: &str) -> Option<&TrackDefinition> {
        self.0.get(name)
    }
}

impl Default for TrackDefinitions {
    fn default() -> Self {
        TrackDefinitions(IndexMap::new())
    }
}

#[derive(Clone)]
pub(crate) struct TrackOccupancy {
    pub(crate) pin_occupancies: FixedBitSet,
    pub(crate) keepout_occupancies: FixedBitSet,
}

impl TrackOccupancy {
    /// Creates a new occupancy bitmap capable of tracking `num_tracks` track
    /// slots for both pins and keepouts.
    pub fn new(num_tracks: usize) -> Self {
        TrackOccupancy {
            pin_occupancies: FixedBitSet::with_capacity(num_tracks),
            keepout_occupancies: FixedBitSet::with_capacity(num_tracks),
        }
    }

    /// Validate that a pin can be placed on tracks in the half-open span
    /// [min_index, max_index].
    pub fn check_place_pin(&self, min_index: i64, max_index: i64) -> Result<(), PinPlacementError> {
        if min_index < 0 || max_index < 0 || (max_index as usize) >= self.pin_occupancies.len() {
            return Err(PinPlacementError::OutOfBounds {
                min_index,
                max_index,
                num_tracks: self.pin_occupancies.len(),
            });
        }
        let min_index_usize = min_index as usize;
        let max_index_usize = max_index as usize;
        let range = min_index_usize..(max_index_usize + 1);
        if self.pin_occupancies.contains_any_in_range(range.clone()) {
            return Err(PinPlacementError::OverlapsExistingPin {
                min_index,
                max_index,
            });
        }
        if self.keepout_occupancies.contains_any_in_range(range) {
            return Err(PinPlacementError::OverlapsKeepout {
                min_index,
                max_index,
            });
        }
        Ok(())
    }

    /// Validate that a keepout can be placed. Keepouts are clipped to the edge
    /// span.
    pub fn check_place_keepout(
        &self,
        min_index: i64,
        max_index: i64,
    ) -> Result<(), PinPlacementError> {
        if max_index < 0 {
            // Entirely before the first track after clipping; trivially OK.
            return Ok(());
        }
        let clipped_min_index = min_index.max(0) as usize;
        let clipped_max_index = max_index.min((self.pin_occupancies.len() as i64) - 1) as usize;
        let range = clipped_min_index..(clipped_max_index + 1);
        if self.pin_occupancies.contains_any_in_range(range) {
            return Err(PinPlacementError::OverlapsExistingPin {
                min_index,
                max_index,
            });
        }
        Ok(())
    }

    /// Marks the inclusive range `[min_index, max_index]` as occupied by a
    /// pin, preventing future placements from overlapping.
    pub fn mark_pin(&mut self, min_index: i64, max_index: i64) {
        let min_index = min_index as usize;
        let max_index = max_index as usize;
        self.pin_occupancies
            .set_range(min_index..(max_index + 1), true);
    }

    /// Marks the inclusive range `[min_index, max_index]` as a keepout region,
    /// clipping indices that fall outside the known edge span.
    pub fn mark_keepout(&mut self, min_index: i64, max_index: i64) {
        let max_index = if max_index < 0 {
            return;
        } else {
            max_index as usize
        };
        let clipped_max_index = max_index.min(self.pin_occupancies.len() - 1);
        let clipped_min_index = if min_index < 0 { 0 } else { min_index as usize };
        self.keepout_occupancies
            .set_range(clipped_min_index..(clipped_max_index + 1), true);
    }

    /// Convenience helper that records both the pin and keepout ranges and
    /// ensures the pin body itself is not treated as a keepout afterwards.
    pub fn place_pin_and_keepout(
        &mut self,
        pin_min_index: i64,
        pin_max_index: i64,
        keepout_min_index: i64,
        keepout_max_index: i64,
    ) {
        self.mark_pin(pin_min_index, pin_max_index);
        self.mark_keepout(keepout_min_index, keepout_max_index);

        let pin_range = (pin_min_index as usize)..((pin_max_index as usize) + 1);
        self.keepout_occupancies.remove_range(pin_range);
    }

    /// Returns a bitmap of tracks that are currently free between
    /// `min_index` and `max_index`, inclusive.
    pub fn get_available_indices_in_range(
        &self,
        min_index: usize,
        max_index: usize,
    ) -> FixedBitSet {
        let mut retval = self.pin_occupancies.clone();
        retval.union_with(&self.keepout_occupancies);
        if min_index > 0 {
            retval.insert_range(..min_index);
        }
        if max_index < (retval.len() - 1) {
            retval.insert_range((max_index + 1)..);
        }
        retval.toggle_range(..);
        retval
    }
}

pub(crate) struct TrackOccupancies(pub(crate) Vec<IndexMap<String, TrackOccupancy>>);

impl TrackOccupancies {
    /// Allocates per-edge occupancy maps for the provided number of edges.
    pub fn new(num_edges: usize) -> Self {
        let mut occupancies = Vec::with_capacity(num_edges);
        for _ in 0..num_edges {
            occupancies.push(IndexMap::new());
        }
        TrackOccupancies(occupancies)
    }

    /// Returns the immutable occupancy record for `edge_index` and `layer`, if
    /// one has been initialized.
    pub fn get_occupancy(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
    ) -> Option<&TrackOccupancy> {
        self.0
            .get(edge_index)
            .and_then(|edge_map| edge_map.get(layer.as_ref()))
    }

    /// Returns the mutable occupancy record for `edge_index` and `layer`, if
    /// one has been initialized.
    pub fn get_occupancy_mut(
        &mut self,
        edge_index: usize,
        layer: impl AsRef<str>,
    ) -> Option<&mut TrackOccupancy> {
        self.0
            .get_mut(edge_index)
            .and_then(|edge_map| edge_map.get_mut(layer.as_ref()))
    }
}

macro_rules! can_place_pin_on_edge {
    ($fn_name:ident, $const_name:ident) => {
        #[doc = concat!(
                                            "Returns `true` when a pin can be placed on the ",
                                            stringify!($fn_name),
                                            " edge for the requested layer and track index."
                                        )]
        pub fn $fn_name(&self, layer: impl AsRef<str>, track_index: usize) -> bool {
            self.can_place_pin_on_edge_index($const_name, layer, track_index)
        }
    };
}

impl ModDef {
    /// Convert a track index on a given edge and layer into an absolute
    /// position and transform that orients a pin polygon to the edge
    /// orientation.
    pub fn track_index_to_position_and_transform(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
    ) -> (Coordinate, Mat3) {
        let layer_ref = layer.as_ref();
        let track = self
            .get_track(layer_ref)
            .unwrap_or_else(|| panic!("Unknown track layer '{layer_ref}'"));
        let edge = self
            .get_edge(edge_index)
            .unwrap_or_else(|| panic!("Edge index {edge_index} is out of bounds"));
        let position = edge.get_coordinate_on_edge(&track, track_index);
        let transform = match edge.orientation() {
            Some(EdgeOrientation::North) => Mat3::identity(),
            Some(EdgeOrientation::South) => Mat3::from_orientation(Orientation::R180),
            Some(EdgeOrientation::East) => Mat3::from_orientation(Orientation::R270),
            Some(EdgeOrientation::West) => Mat3::from_orientation(Orientation::R90),
            None => panic!("Edge is not axis-aligned; only rectilinear edges are supported"),
        };
        (position, transform)
    }

    pub(crate) fn track_range_for_polygon(
        &self,
        layer: impl AsRef<str>,
        track_index: usize,
        polygon: &Polygon,
    ) -> (i64, i64) {
        let track = self.get_track(layer.as_ref()).unwrap();
        let bbox = polygon.bbox();
        let (min_delta, max_delta) = match track.orientation {
            TrackOrientation::Horizontal => (bbox.min_y, bbox.max_y),
            TrackOrientation::Vertical => (bbox.min_x, bbox.max_x),
        };
        let tracks_below = min_delta / track.period;
        let tracks_above = max_delta / track.period;
        (
            (track_index as i64) + tracks_below,
            (track_index as i64) + tracks_above,
        )
    }

    can_place_pin_on_edge!(can_place_pin_on_west_edge, WEST_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_left_edge, LEFT_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_north_edge, NORTH_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_top_edge, TOP_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_east_edge, EAST_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_right_edge, RIGHT_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_south_edge, SOUTH_EDGE_INDEX);
    can_place_pin_on_edge!(can_place_pin_on_bottom_edge, BOTTOM_EDGE_INDEX);

    /// Boolean convenience wrapper around
    /// [`ModDef::check_pin_placement_on_edge_index`].
    pub fn can_place_pin_on_edge_index(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
    ) -> bool {
        let track = self.get_track(layer.as_ref()).unwrap();
        self.check_pin_placement_on_edge_index_with_polygon(
            edge_index,
            layer,
            track_index,
            track.pin_shape.as_ref(),
            track.keepout_shape.as_ref(),
        )
        .is_ok()
    }

    /// Validate that a pin/keepout combination can be placed using the default
    /// shapes for the layer, returning `Ok(())` on success or a detailed
    /// [`PinPlacementError`] on failure.
    pub fn check_pin_placement_on_edge_index(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
    ) -> Result<(), PinPlacementError> {
        let track = self.get_track(layer.as_ref()).unwrap();
        self.check_pin_placement_on_edge_index_with_polygon(
            edge_index,
            layer,
            track_index,
            track.pin_shape.as_ref(),
            track.keepout_shape.as_ref(),
        )
    }

    /// Validate that a pin/keepout combination can be placed using the provided
    /// polygons, returning `Ok(())` on success or a detailed
    /// [`PinPlacementError`] on failure.
    pub fn check_pin_placement_on_edge_index_with_polygon(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
        pin_polygon: Option<&Polygon>,
        keepout_polygon: Option<&Polygon>,
    ) -> Result<(), PinPlacementError> {
        let core = self.core.borrow();
        let occupancies = core
            .track_occupancies
            .as_ref()
            .ok_or(PinPlacementError::NotInitialized("Track occupancies"))?;
        let num_edges = occupancies.0.len();
        let edge_map = occupancies
            .0
            .get(edge_index)
            .ok_or(PinPlacementError::EdgeOutOfBounds {
                edge_index,
                num_edges,
            })?;
        let layer_ref = layer.as_ref();
        let occupancy =
            edge_map
                .get(layer_ref)
                .ok_or_else(|| PinPlacementError::LayerUnavailable {
                    layer: layer_ref.to_string(),
                })?;

        if let Some(pin_polygon) = pin_polygon {
            let (min_track_index, max_track_index) =
                self.track_range_for_polygon(layer_ref, track_index, pin_polygon);
            occupancy.check_place_pin(min_track_index, max_track_index)?;
        }

        if let Some(keepout_polygon) = keepout_polygon {
            let (min_track_index, max_track_index) =
                self.track_range_for_polygon(layer_ref, track_index, keepout_polygon);
            occupancy.check_place_keepout(min_track_index, max_track_index)?;
        }

        Ok(())
    }
}
