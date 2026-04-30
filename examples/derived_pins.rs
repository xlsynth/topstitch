// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};
use topstitch::{
    BoundingBox, Coordinate, LefDefOptions, ModDef, ModInst, Orientation, Polygon, Port, PortSlice,
    Range, SpreadPinsOptions, TrackDefinition, TrackDefinitions, TrackOrientation, Usage,
};

const WIDTH: i64 = 100;
const HEIGHT: i64 = 4 * PIN_WIDTH * (NUM_PINS as i64);
const NUM_PINS: usize = 2;
const PIN_WIDTH: i64 = 10;
const PIN_DEPTH: i64 = 20;

fn build_leaf(
    name: &str,
    track_definitions: &TrackDefinitions,
    shape: Polygon,
) -> Result<ModDef, Box<dyn std::error::Error>> {
    let m = ModDef::new(name);
    m.set_shape(shape);
    m.set_track_definitions(track_definitions.clone());

    m.feedthrough("in", "out", NUM_PINS);

    // Mark as a leaf for LEF/DEF emission
    m.set_usage(Usage::EmitStubAndStop);
    Ok(m)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    // Track definitions
    let mut track_definitions = TrackDefinitions::new();
    let pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: -PIN_WIDTH / 2,
        min_y: 0,
        max_x: PIN_WIDTH / 2,
        max_y: PIN_DEPTH,
    });
    track_definitions.add_track(TrackDefinition::new(
        "M1",
        PIN_WIDTH,
        2 * PIN_WIDTH,
        TrackOrientation::Horizontal,
        Some(pin_shape.clone()),
        None,
    ));

    // Build A and B
    let a = build_leaf(
        "A",
        &track_definitions,
        Polygon::from_width_height(WIDTH, HEIGHT),
    )?;
    let b = build_leaf(
        "B",
        &track_definitions,
        Polygon::new(vec![
            Coordinate { x: 0, y: 0 },
            Coordinate {
                x: 0,
                y: 5 * HEIGHT / 4,
            },
            Coordinate {
                x: WIDTH / 2,
                y: 5 * HEIGHT / 4,
            },
            Coordinate {
                x: WIDTH / 2,
                y: HEIGHT,
            },
            Coordinate {
                x: WIDTH,
                y: HEIGHT,
            },
            Coordinate { x: WIDTH, y: 0 },
        ]),
    )?;

    let top = ModDef::new("Top");
    let a_instance = top.instantiate(&a, Some("a_inst"), None);
    let b_instance = top.instantiate(&b, Some("b_inst"), None);

    // Coordinates chosen to cover positive and negative x and y values
    a_instance.place((-40, -40), Orientation::R0);
    b_instance.place(((2 * WIDTH) - 20, -20), Orientation::MY);

    // Pin inputs and outputs
    a.get_port("in").spread_pins_on_left_edge(
        a.get_layers(),
        SpreadPinsOptions {
            range: Range::new(HEIGHT / 4, 3 * HEIGHT / 4),
            ..Default::default()
        },
    )?;

    let a_in = a_instance.get_port("in");
    let a_out = a_instance.get_port("out");
    let b_in = b_instance.get_port("in");
    let b_out = b_instance.get_port("out");

    b_in.connect(&a_out);

    a_out.place_across_from(a_in);
    b_in.place_abutted();
    b_out.place_across_from(b_in);

    // Write a CSV report of physical connection distances. The report code
    // below starts at the supplied top module, walks every instance
    // recursively, checks each bit of each selected physical leaf port, and
    // merges adjacent bits back into slices when they have the same distance.
    let csv_path = out_dir.join("derived_pins_connection_distances.csv");
    write_connection_distance_report(&top, &csv_path)?;

    // Emit LEF/DEF for viewing
    let lef_path = out_dir.join("derived_pins.lef");
    let def_path = out_dir.join("derived_pins.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &LefDefOptions::default())
        .expect("emit LEF/DEF");

    Ok(())
}

fn write_connection_distance_report(top: &ModDef, path: &Path) -> io::Result<()> {
    let mut report = File::create(path)?;
    writeln!(report, "source,connected,distance")?;

    for instance in top.get_instances() {
        write_instance_connection_distances(&instance, &mut report)?;
    }

    Ok(())
}

fn write_instance_connection_distances(
    instance: &ModInst,
    report: &mut dyn Write,
) -> io::Result<()> {
    // Only physical leaf modules have meaningful leaf pins in this example.
    // Additional report filters could be added here, for example by matching
    // module names, instance names, or port names before iterating ports.
    if instance.get_mod_def().get_usage() == Usage::EmitStubAndStop {
        for port in instance.get_ports(None) {
            write_port_connection_distances(&port, report)?;
        }
    }

    for child in instance.get_instances() {
        write_instance_connection_distances(&child, report)?;
    }

    Ok(())
}

fn write_port_connection_distances(port: &Port, report: &mut dyn Write) -> io::Result<()> {
    let mut runs: Vec<ConnectionDistanceRun> = Vec::new();

    for bit in 0..port.io().width() {
        let source = port.bit(bit);
        let Some((connected, distance)) = source.get_connected_port_slice_and_distance() else {
            continue;
        };

        if let Some(run) = runs.last_mut()
            && run.try_extend(&source, &connected, distance)
        {
            continue;
        }

        runs.push(ConnectionDistanceRun::new(&source, &connected, distance));
    }

    for run in runs {
        write_connection_distance_run(&run, report)?;
    }

    Ok(())
}

fn write_connection_distance_run(
    run: &ConnectionDistanceRun,
    report: &mut dyn Write,
) -> io::Result<()> {
    let source = run.source_port.slice(run.source_msb, run.source_lsb);
    let connected = run
        .connected_port
        .slice(run.connected_msb, run.connected_lsb);
    let source_name = format_port_slice_for_report(&source);
    let connected_name = format_port_slice_for_report(&connected);
    writeln!(
        report,
        "{},{},{}",
        csv_field(&source_name),
        csv_field(&connected_name),
        run.distance
    )
}

struct ConnectionDistanceRun {
    source_port: Port,
    source_lsb: usize,
    source_msb: usize,
    connected_port: Port,
    connected_lsb: usize,
    connected_msb: usize,
    connected_last_bit: usize,
    connected_step: Option<isize>,
    distance: i64,
}

impl ConnectionDistanceRun {
    fn new(source: &PortSlice, connected: &PortSlice, distance: i64) -> Self {
        let connected_bit = connected.lsb();
        Self {
            source_port: source.get_port(),
            source_lsb: source.lsb(),
            source_msb: source.msb(),
            connected_port: connected.get_port(),
            connected_lsb: connected_bit,
            connected_msb: connected_bit,
            connected_last_bit: connected_bit,
            connected_step: None,
            distance,
        }
    }

    fn try_extend(&mut self, source: &PortSlice, connected: &PortSlice, distance: i64) -> bool {
        if source.lsb() != self.source_msb + 1
            || source.get_port() != self.source_port
            || connected.get_port() != self.connected_port
            || distance != self.distance
        {
            return false;
        }

        let connected_bit = connected.lsb();
        let step = connected_bit as isize - self.connected_last_bit as isize;
        if !matches!(step, -1 | 1) {
            return false;
        }

        if let Some(existing_step) = self.connected_step
            && step != existing_step
        {
            return false;
        }

        self.source_msb = source.msb();
        self.connected_lsb = self.connected_lsb.min(connected_bit);
        self.connected_msb = self.connected_msb.max(connected_bit);
        self.connected_last_bit = connected_bit;
        self.connected_step = Some(step);
        true
    }
}

fn format_port_slice_for_report(port_slice: &PortSlice) -> String {
    let name = format!("{port_slice:?}");
    if port_slice.lsb() == port_slice.msb() {
        let bit_suffix = format!("[{}:{}]", port_slice.lsb(), port_slice.lsb());
        if let Some(prefix) = name.strip_suffix(&bit_suffix) {
            return format!("{prefix}[{}]", port_slice.lsb());
        }
    }
    name
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
