// SPDX-License-Identifier: Apache-2.0

use indexmap::{map::Entry, IndexMap};
use std::collections::{HashMap, HashSet};

use crate::mod_def::dtypes::{Coordinate, PhysicalPin, Polygon, Range};
use crate::{for_each_edge_direction, ModDef, Port, PortSlice};

macro_rules! place_pin_on_named_edge {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
                #[doc = concat!(
                    "Places the specified pin bit on the ",
                    stringify!($edge_name),
                    " edge using the default track definition."
                )]
                pub fn [<place_pin_on_ $edge_name _edge>](
                &self,
                port_name: impl AsRef<str>,
                bit: usize,
                layer: impl AsRef<str>,
                track_index: usize
            ) {
                assert!(
                    self.shape_is_rectangular(),
                    "Cannot use cardinal direction names for edges for a non-rectangular shape"
                );
                self.place_pin_on_edge_index(port_name, bit, $const_name, layer, track_index);
            }
        }
    };
}

macro_rules! place_pins_on_named_edge_index {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Places the provided pins on the ",
                stringify!($edge_name),
                " edge while honoring optional spacing and layer priorities."
            )]
            pub fn [<place_pins_on_ $edge_name _edge>]<L, S>(
                &self,
                pins: &[(impl AsRef<str>, usize)],
                layers: L,
                position_range: Range,
                min_spacing: Option<i64>,
            ) -> Result<(), BatchPinPlacementError>
            where
                L: IntoIterator<Item = S>,
                S: AsRef<str>,
            {
                self.place_pins_on_edge_index(
                    pins,
                    $const_name,
                    layers,
                    position_range,
                    min_spacing,
                )
            }
        }
    };
}

macro_rules! place_pins_on_named_edge_index_with_polygons {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Places the provided pins on the ",
                stringify!($edge_name),
                " edge using explicit pin and keepout polygons per layer."
            )]
            pub fn [<place_pins_on_ $edge_name _edge_with_polygons>](
                &self,
                pins: &[(impl AsRef<str>, usize)],
                layers: IndexMap<String, (Polygon, Option<Polygon>)>,
                position_range: Range,
                min_spacing: Option<i64>,
            ) -> Result<(), BatchPinPlacementError> {
                self.place_pins_on_edge_index_with_polygons(
                    pins,
                    $const_name,
                    layers,
                    position_range,
                    min_spacing,
                )
            }
        }
    };
}

macro_rules! spread_pins_on_named_edge_index {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Evenly spreads the provided pins across the ",
                stringify!($edge_name),
                " edge using layer defaults."
            )]
            pub fn [<spread_pins_on_ $edge_name _edge>]<L, S>(
                &self,
                pins: &[(impl AsRef<str>, usize)],
                layers: L,
                position_range: Range,
            ) -> Result<(), BatchPinPlacementError>
            where
                L: IntoIterator<Item = S>,
                S: AsRef<str>,
            {
                self.spread_pins_on_edge_index(
                    pins,
                    $const_name,
                    layers,
                    position_range,
                )
            }
        }
    };
}

macro_rules! spread_pins_on_named_edge_index_with_polygons {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Evenly spreads the provided pins across the ",
                stringify!($edge_name),
                " edge using custom per-layer pin polygons."
            )]
            pub fn [<spread_pins_on_ $edge_name _edge_with_polygons>](
                &self,
                pins: &[(impl AsRef<str>, usize)],
                layers: IndexMap<String, (Polygon, Option<Polygon>)>,
                position_range: Range,
            ) -> Result<(), BatchPinPlacementError> {
                self.spread_pins_on_edge_index_with_polygons(
                    pins,
                    $const_name,
                    layers,
                    position_range,
                )
            }
        }
    };
}

macro_rules! spread_port_pins_on_named_edge {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            impl Port {
                #[doc = concat!(
                    "Spreads the bits of this port on the ",
                    stringify!($edge_name),
                    " edge using layer defaults."
                )]
                pub fn [<spread_pins_on_ $edge_name _edge>]<L, S>(
                    &self,
                    layers: L,
                    position_range: Range,
                ) -> Result<(), BatchPinPlacementError>
                where
                    L: IntoIterator<Item = S>,
                    S: AsRef<str>,
                {
                    let mod_def = ModDef { core: self.get_mod_def_core() };
                    mod_def.[<spread_pins_on_ $edge_name _edge>](&self.to_bits(), layers, position_range)
                }
            }
        }
    };
}

macro_rules! spread_port_slice_pins_on_named_edge {
    ($edge_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Spreads the bits of this `PortSlice` on the ",
                stringify!($edge_name),
                " edge using layer defaults."
            )]
            pub fn [<spread_pins_on_ $edge_name _edge>]<L, S>(
                &self,
                layers: L,
                position_range: Range,
            ) -> Result<(), BatchPinPlacementError>
            where
                L: IntoIterator<Item = S>,
                S: AsRef<str>,
            {
                self.get_mod_def().[<spread_pins_on_ $edge_name _edge>](
                    &self.to_bits(),
                    layers,
                    position_range,
                )
            }
        }
    };
}

/// Describes why a batch pin placement request could not be satisfied.
#[derive(Debug, Clone)]
pub enum BatchPinPlacementError {
    /// There were more pins than available layer slots.
    RanOutOfLayers { requested: usize, placed: usize },
    /// The selected edge index was not valid for the current shape.
    EdgeOutOfBounds { edge_index: usize, num_edges: usize },
    /// The requested coordinate window falls outside the selected edge span.
    RequestOutOfBounds {
        edge_index: usize,
        edge_range: Range,
        req_range: Range,
    },
    /// The requested track indices fell outside the layer coverage for the
    /// edge.
    OffTrackRange {
        layer: String,
        req_range: Range,
        edge_range: Range,
    },
}

impl std::fmt::Display for BatchPinPlacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchPinPlacementError::RanOutOfLayers { requested, placed } => write!(
                f,
                "unable to place all pins: requested {}, placed {} (ran out of layers)",
                requested, placed
            ),
            BatchPinPlacementError::EdgeOutOfBounds {
                edge_index,
                num_edges,
            } => write!(
                f,
                "edge index {} is out of bounds ({} edges available)",
                edge_index, num_edges
            ),
            BatchPinPlacementError::RequestOutOfBounds {
                edge_index,
                edge_range,
                req_range,
            } => write!(
                f,
                "requested coordinate range {} on edge {} lies outside edge span {}",
                req_range, edge_index, edge_range
            ),
            BatchPinPlacementError::OffTrackRange {
                layer,
                req_range,
                edge_range,
            } => write!(
                f,
                "requested absolute track range {} on layer '{}' lies outside edge coverage {}",
                req_range, layer, edge_range
            ),
        }
    }
}

impl std::error::Error for BatchPinPlacementError {}

impl ModDef {
    /// Creates a scratch copy of the module for speculative placement checks.
    fn clone_for_pin_placement(&self) -> ModDef {
        use crate::mod_def::tracks::{TrackOccupancies, TrackOccupancy};
        let core = self.core.borrow();

        // Deep-copy track occupancies if present
        let cloned_occupancies: Option<TrackOccupancies> =
            core.track_occupancies.as_ref().map(|occ| {
                let mut vec_maps: Vec<indexmap::IndexMap<String, TrackOccupancy>> =
                    Vec::with_capacity(occ.0.len());
                for edge_map in occ.0.iter() {
                    let mut new_map = indexmap::IndexMap::new();
                    for (layer, o) in edge_map.iter() {
                        let mut no = TrackOccupancy::new(o.pin_occupancies.len());
                        no.pin_occupancies = o.pin_occupancies.clone();
                        no.keepout_occupancies = o.keepout_occupancies.clone();
                        new_map.insert(layer.clone(), no);
                    }
                    vec_maps.push(new_map);
                }
                TrackOccupancies(vec_maps)
            });

        let new_core = crate::mod_def::ModDefCore {
            name: core.name.clone(),
            ports: core.ports.clone(),
            interfaces: IndexMap::new(),
            instances: IndexMap::new(),
            usage: core.usage.clone(),
            generated_verilog: None,
            verilog_import: None,
            assignments: Vec::new(),
            unused: Vec::new(),
            tieoffs: Vec::new(),
            whole_port_tieoffs: IndexMap::new(),
            whole_port_unused: IndexMap::new(),
            inst_connections: IndexMap::new(),
            reserved_net_definitions: core.reserved_net_definitions.clone(),
            enum_ports: IndexMap::new(),
            adjacency_matrix: HashMap::new(),
            ignore_adjacency: HashSet::new(),
            shape: core.shape.clone(),
            layer: core.layer.clone(),
            inst_placements: IndexMap::new(),
            physical_pins: core.physical_pins.clone(),
            track_definitions: core.track_definitions.clone(),
            track_occupancies: cloned_occupancies,
        };

        ModDef {
            core: std::rc::Rc::new(std::cell::RefCell::new(new_core)),
        }
    }
    /// Define a physical pin for this single-bit PortSlice, with an arbitrary
    /// polygon shape relative to `position` on the given `layer`.
    pub fn place_pin(
        &self,
        port_name: impl AsRef<str>,
        bit: usize,
        layer: impl AsRef<str>,
        position: Coordinate,
        polygon: Polygon,
    ) {
        let mut core = self.core.borrow_mut();
        let io = core.ports.get(port_name.as_ref()).unwrap_or_else(|| {
            panic!(
                "Port {}.{} does not exist (adding physical pin)",
                self.core.borrow().name,
                port_name.as_ref()
            )
        });
        let width = io.width();
        if bit >= width {
            panic!(
                "Bit {} out of range for port {}.{} with width {}",
                bit,
                self.core.borrow().name,
                port_name.as_ref(),
                width
            );
        }

        // Ensure vector of appropriate width exists on first use
        let pins_for_port = match core.physical_pins.entry(port_name.as_ref().to_string()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(v) => v.insert(vec![None; width]),
        };

        pins_for_port[bit] = Some(PhysicalPin {
            layer: layer.as_ref().to_string(),
            position,
            polygon,
        });
    }

    for_each_edge_direction!(place_pin_on_named_edge);
    for_each_edge_direction!(place_pins_on_named_edge_index);
    for_each_edge_direction!(place_pins_on_named_edge_index_with_polygons);
    for_each_edge_direction!(spread_pins_on_named_edge_index);
    for_each_edge_direction!(spread_pins_on_named_edge_index_with_polygons);

    /// Define a physical pin for this single-bit `PortSlice` on a specific edge
    /// by index, using the default pin/keepout shapes from the layer's
    /// track definition.
    pub fn place_pin_on_edge_index(
        &self,
        port_name: impl AsRef<str>,
        bit: usize,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
    ) {
        let track = self.get_track(layer.as_ref()).unwrap();
        self.place_pin_on_edge_index_with_polygon(
            port_name,
            bit,
            edge_index,
            layer,
            track_index,
            track.pin_shape.as_ref(),
            track.keepout_shape.as_ref(),
        );
    }

    /// Define a physical pin for this single-bit `PortSlice` on a specific edge
    /// by index, using the provided pin/keepout polygons (relative to the
    /// track origin). Panics with a descriptive message if the placement is
    /// not allowed.
    #[allow(clippy::too_many_arguments)]
    pub fn place_pin_on_edge_index_with_polygon(
        &self,
        port_name: impl AsRef<str>,
        bit: usize,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
        pin_polygon: Option<&Polygon>,
        keepout_polygon: Option<&Polygon>,
    ) {
        let layer_ref = layer.as_ref();

        // Validate placement and surface a precise error message if disallowed
        if let Err(err) = self.check_pin_placement_on_edge_index_with_polygon(
            edge_index,
            layer_ref,
            track_index,
            pin_polygon,
            keepout_polygon,
        ) {
            panic!(
                "Cannot place pin for {}.{}[{}] on edge {} (layer '{}', track {}): {}",
                self.core.borrow().name,
                port_name.as_ref(),
                bit,
                edge_index,
                layer_ref,
                track_index,
                err
            );
        }

        if let Some(pin_polygon) = pin_polygon {
            let (pin_min_track, pin_max_track) =
                self.track_range_for_polygon(layer_ref, track_index, pin_polygon);
            if let Some(keepout_polygon) = keepout_polygon {
                let (keepout_min_track, keepout_max_track) =
                    self.track_range_for_polygon(layer_ref, track_index, keepout_polygon);
                self.mark_pin_and_keepout_ranges(
                    edge_index,
                    layer_ref,
                    pin_min_track,
                    pin_max_track,
                    keepout_min_track,
                    keepout_max_track,
                );
            } else {
                let (pin_min_track, pin_max_track) =
                    self.track_range_for_polygon(layer_ref, track_index, pin_polygon);
                self.mark_pin_range(edge_index, layer_ref, pin_min_track, pin_max_track);
            }

            let (position, transform) =
                self.track_index_to_position_and_transform(edge_index, layer_ref, track_index);
            let pin_polygon = pin_polygon.apply_transform(&transform);
            self.place_pin(port_name, bit, layer_ref, position, pin_polygon);
        } else if let Some(keepout_polygon) = keepout_polygon {
            let (keepout_min_track, keepout_max_track) =
                self.track_range_for_polygon(layer_ref, track_index, keepout_polygon);
            self.mark_keepout_range(edge_index, layer_ref, keepout_min_track, keepout_max_track);
        }
    }

    /// Places each `(port, bit)` on `edge_index` using `layers` in priority
    /// order, optionally enforcing a minimum track spacing.
    pub fn place_pins_on_edge_index<L, S>(
        &self,
        pins: &[(impl AsRef<str>, usize)],
        edge_index: usize,
        layers: L,
        position_range: Range,
        min_spacing: Option<i64>,
    ) -> Result<(), BatchPinPlacementError>
    where
        L: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.place_pins_on_edge_index_with_polygons(
            pins,
            edge_index,
            self.get_default_layer_shapes(layers),
            position_range,
            min_spacing,
        )
    }

    /// Places each `(port, bit)` on `edge_index` using explicit pin/keepout
    /// shapes provided per layer.
    pub fn place_pins_on_edge_index_with_polygons(
        &self,
        pins: &[(impl AsRef<str>, usize)],
        edge_index: usize,
        layers: IndexMap<String, (Polygon, Option<Polygon>)>,
        position_range: Range,
        min_spacing: Option<i64>,
    ) -> Result<(), BatchPinPlacementError> {
        let mut placed_count: usize = 0;

        // find range of coordinates for this edge
        let edge = match self.get_edge(edge_index) {
            Some(e) => e,
            None => {
                return Err(BatchPinPlacementError::EdgeOutOfBounds {
                    edge_index,
                    num_edges: self.get_num_edges(),
                })
            }
        };
        let edge_range = match edge.get_coord_range() {
            Some(v) => v,
            None => {
                return Err(BatchPinPlacementError::EdgeOutOfBounds {
                    edge_index,
                    num_edges: self.get_num_edges(),
                })
            }
        };
        let edge_min = edge_range.min.unwrap();
        let req_abs_range = Range {
            min: position_range.min.map(|v| edge_min + v),
            max: position_range.max.map(|v| edge_min + v),
        };
        if !req_abs_range.is_subset_of(&edge_range) {
            return Err(BatchPinPlacementError::RequestOutOfBounds {
                edge_index,
                edge_range,
                req_range: req_abs_range,
            });
        }

        let req_range = req_abs_range;

        // Build candidate list: (absolute param along edge, layer priority index, track
        // index on edge)
        struct Candidate {
            position: i64,
            layer_idx: usize,
            track_index: usize,
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        let mut spacing_by_layer: Vec<Option<i64>> = Vec::new();

        // Maintain a side table of layer names in insertion order
        let layer_names: Vec<&str> = layers.keys().map(|k| k.as_str()).collect();

        let edge_orientation = edge
            .orientation()
            .expect("Edge orientation must be rectilinear");
        for (layer_idx, layer_name) in layer_names.iter().enumerate() {
            // track def
            let track = match self.get_track(layer_name) {
                Some(t) => t,
                None => continue,
            };

            if !track
                .orientation
                .is_compatible_with_edge_orientation(&edge_orientation)
            {
                // need an empty entry for this layer to avoid offset in indices
                spacing_by_layer.push(None);
                continue;
            }

            // quantized request window to track indices within this edge coverage
            let req_tracks = track.convert_coord_range_to_index_range(&req_range);
            let edge_tracks = match edge.get_index_range(&track) {
                Some(v) => v,
                None => continue,
            };
            let edge_min_index = match edge_tracks.min {
                Some(v) => v,
                None => continue,
            };
            let (rel_min, rel_max) = match req_tracks.intersection(&edge_tracks) {
                Some(Range {
                    min: Some(min),
                    max: Some(max),
                }) => (min - edge_min_index, max - edge_min_index),
                _ => continue,
            };

            assert!(rel_min >= 0);
            let rel_min = rel_min as usize;

            assert!(rel_max >= 0);
            let rel_max = rel_max as usize;

            // spacing expressed in tracks for this layer
            let spacing_tracks =
                min_spacing.map(|spacing| (spacing + track.period - 1) / track.period);
            spacing_by_layer.push(spacing_tracks);

            // Collect all candidate track indices in the requested window
            // Candidate tracks are those not occupied by pins or keepouts
            let track_occupancy = match self.get_occupancy(edge_index, layer_name) {
                Some(v) => v,
                None => continue,
            };
            candidates.extend(
                track_occupancy
                    .get_available_indices_in_range(rel_min, rel_max)
                    .ones()
                    .map(|track_index| {
                        let position = edge.get_position_on_edge(&track, track_index);
                        Candidate {
                            position,
                            layer_idx,
                            track_index,
                        }
                    }),
            );
        }

        // Sort by absolute param; tie-break by layer priority then track index
        candidates.sort_by(|a, b| {
            use std::cmp::Ordering;
            match a.position.cmp(&b.position) {
                Ordering::Equal => match a.layer_idx.cmp(&b.layer_idx) {
                    Ordering::Equal => a.track_index.cmp(&b.track_index),
                    other => other,
                },
                other => other,
            }
        });

        // Per-layer last placed track index to enforce spacing
        let mut last_idx_by_layer: Vec<Option<i64>> = vec![None; spacing_by_layer.len()];

        // Iterate candidates; place until we run out of pins
        for c in candidates.into_iter() {
            if placed_count >= pins.len() {
                break;
            }

            // spacing check (per layer)
            if let (Some(prev), Some(min_sp)) = (
                last_idx_by_layer[c.layer_idx],
                spacing_by_layer[c.layer_idx],
            ) {
                let current_rel = c.track_index as i64;
                if current_rel - prev < min_sp {
                    continue;
                }
            }

            // Identify layer name by priority index
            let (layer_name, _) = layers
                .get_index(c.layer_idx)
                .expect("layer index out of bounds");

            // Check if
            let layer_shapes = layers.get(layer_name).unwrap();
            if self
                .check_pin_placement_on_edge_index_with_polygon(
                    edge_index,
                    layer_name,
                    c.track_index,
                    Some(&layer_shapes.0),
                    layer_shapes.1.as_ref(),
                )
                .is_err()
            {
                continue;
            }

            let (port_name, bit) = (
                pins[placed_count].0.as_ref().to_string(),
                pins[placed_count].1,
            );

            self.place_pin_on_edge_index(
                port_name,
                bit,
                edge_index,
                layer_name.as_str(),
                c.track_index,
            );

            last_idx_by_layer[c.layer_idx] = Some(c.track_index as i64);
            placed_count += 1;
        }

        if placed_count == pins.len() {
            Ok(())
        } else {
            Err(BatchPinPlacementError::RanOutOfLayers {
                requested: pins.len(),
                placed: placed_count,
            })
        }
    }

    /// Find the largest uniform spacing that still allows placing all pins,
    /// then place them. Returns the chosen spacing (in edge-parallel
    /// coordinate units).
    pub fn spread_pins_on_edge_index_with_polygons(
        &self,
        pins: &[(impl AsRef<str>, usize)],
        edge_index: usize,
        layers: IndexMap<String, (Polygon, Option<Polygon>)>,
        position_range: Range,
    ) -> Result<(), BatchPinPlacementError> {
        let search_span = match (position_range.min, position_range.max) {
            (Some(a), Some(b)) => (b - a).max(0),
            _ => {
                // For open-ended ranges, pick a conservative span based on edge range
                let edge = self.get_edge(edge_index).unwrap();
                let er = edge.get_coord_range().unwrap();
                let a = position_range.min.unwrap_or(0);
                let b = position_range
                    .max
                    .unwrap_or(er.max.unwrap() - er.min.unwrap());
                (b - a).max(0)
            }
        };

        let spacing_works = |spacing: i64| -> bool {
            let sim = self.clone_for_pin_placement();
            sim.place_pins_on_edge_index_with_polygons(
                pins,
                edge_index,
                layers.clone(),
                Range {
                    min: position_range.min,
                    max: position_range.max,
                },
                Some(spacing.max(0)),
            )
            .is_ok()
        };

        // Binary search for maximum spacing in [0, search_span]
        // Structured such that at any give iteration, "lo" is a
        // tested spacing that works, but "hi" has not been tested.
        // The exception is lo=0, which ends up being tested at
        // the end if no larger spacings work. If lo=0 doesn't work
        // either, then no spacings will work so the code panics.
        let mut lo: i64 = 0;
        let mut hi: i64 = search_span;
        while lo < hi {
            // if hi=lo+1, mid will be "hi", i.e. the untested spacing
            let mid = (lo + hi + 1) / 2;
            if spacing_works(mid) {
                lo = mid; // search upper half
            } else {
                hi = mid - 1; // search lower half
            }
        }
        let best = lo;

        // Place for real using the best spacing
        self.place_pins_on_edge_index_with_polygons(
            pins,
            edge_index,
            layers,
            position_range,
            Some(best),
        )?;
        Ok(())
    }

    /// Convenience wrapper building layer shapes from default track
    /// definitions.
    pub fn spread_pins_on_edge_index<L, S>(
        &self,
        pins: &[(impl AsRef<str>, usize)],
        edge_index: usize,
        layers: L,
        position_range: Range,
    ) -> Result<(), BatchPinPlacementError>
    where
        L: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.spread_pins_on_edge_index_with_polygons(
            pins,
            edge_index,
            self.get_default_layer_shapes(layers),
            position_range,
        )
    }

    fn get_default_layer_shapes<L, S>(
        &self,
        layers: L,
    ) -> IndexMap<String, (Polygon, Option<Polygon>)>
    where
        L: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut layers_map: IndexMap<String, (Polygon, Option<Polygon>)> = IndexMap::new();
        for l in layers.into_iter() {
            let name = l.as_ref();
            if let Some(track) = self.get_track(name) {
                if let Some(pin_shape) = track.pin_shape.clone() {
                    layers_map.insert(name.to_string(), (pin_shape, track.keepout_shape.clone()));
                }
            }
        }
        layers_map
    }
}

macro_rules! place_port_slice_on_named_edge {
    ($fn_name:ident, $const_name:path) => {
        paste::paste! {
            #[doc = concat!(
                "Places this single-bit slice on the ",
                stringify!($fn_name),
                " edge using the default track definition."
            )]
            pub fn [<place_on_ $fn_name _edge>](&self, layer: impl AsRef<str>, track_index: usize) {
                let (port_name, bit) = self.get_port_name_and_bit();
                self.get_mod_def().[<place_pin_on_ $fn_name _edge>](port_name, bit, layer, track_index);
            }
        }
    };
}

impl PortSlice {
    fn get_port_name_and_bit(&self) -> (String, usize) {
        self.check_validity();
        assert!(
            self.width() == 1,
            "define_physical_pin must be called on a single bit slice"
        );
        // Only allowed on ModDef ports (not instance ports)
        assert!(
            matches!(self.port, crate::Port::ModDef { .. }),
            "define_physical_pin must be called on a ModDef port"
        );

        let port_name = self.port.get_port_name();
        let bit = self.lsb; // since width()==1
        (port_name, bit)
    }

    /// Define a physical pin for this single-bit PortSlice, with an arbitrary
    /// polygon shape relative to `position` on the given `layer`.
    pub fn place(&self, layer: impl AsRef<str>, position: Coordinate, polygon: Polygon) {
        let (port_name, bit) = self.get_port_name_and_bit();
        self.get_mod_def()
            .place_pin(port_name, bit, layer, position, polygon);
    }

    for_each_edge_direction!(place_port_slice_on_named_edge);
    for_each_edge_direction!(spread_port_slice_pins_on_named_edge);

    /// Define a physical pin for this single-bit `PortSlice` on a specific edge
    /// by index, using the default pin/keepout shapes from the layer's
    /// track definition.
    pub fn place_on_edge_index(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
    ) {
        let (port_name, bit) = self.get_port_name_and_bit();
        self.get_mod_def()
            .place_pin_on_edge_index(port_name, bit, edge_index, layer, track_index);
    }

    /// Define a physical pin for this single-bit `PortSlice` on a specific edge
    /// by index, using the provided pin/keepout polygons (relative to the
    /// track origin). Panics with a descriptive message if the placement is
    /// not allowed.
    pub fn place_on_edge_index_with_polygon(
        &self,
        edge_index: usize,
        layer: impl AsRef<str>,
        track_index: usize,
        pin_polygon: Option<&Polygon>,
        keepout_polygon: Option<&Polygon>,
    ) {
        let (port_name, bit) = self.get_port_name_and_bit();
        self.get_mod_def().place_pin_on_edge_index_with_polygon(
            port_name,
            bit,
            edge_index,
            layer,
            track_index,
            pin_polygon,
            keepout_polygon,
        );
    }
}

// Generate Port edge helpers
for_each_edge_direction!(spread_port_pins_on_named_edge);
