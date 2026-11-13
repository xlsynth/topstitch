---
name: topstitch
description: Use when writing or debugging a Rust generator that uses TopStitch to connect Verilog modules, export ports or interfaces, route a feedthrough, or place blocks and pins.
---

# TopStitch generators

Check which TopStitch version the generator builds against. For a Cargo project, `cargo tree -i topstitch` shows the selected version and its callers. Use that version's API documentation when a method differs from a guide example.

- [First generator](references/first-design.md) for importing RTL, specializing a parameter, and emitting a top module.
- [Ports and interfaces](references/connections.md) for matching signal groups, exporting child ports, and splitting buses.
- [Module hierarchy](references/hierarchy.md) for shared definitions, wrappers, and choosing which module bodies to emit.
- [Feedthroughs](references/feedthroughs.md) for passing a signal through a generated module or introducing clocked latency.
- [Placement and pins](references/placement.md) for choosing a reference frame, spreading pins, and aligning connected pins.
- [Inspect and trace connections](references/debugging.md) to find uncovered bits, inspect direct connections, or trace a signal through module boundaries.

For a wiring edit, identify the module that owns the connection and trace both ends of each affected slice. Run the project's generator and inspect the parent ports and instance connections in its output. If the edit changes signal timing, exercise the affected behavior in a testbench. For a placement edit, check the block orientation and pin geometry in the generated physical views.
