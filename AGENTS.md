# Working on TopStitch

Use [CONTRIBUTING.md](CONTRIBUTING.md) for checkout setup, test commands, and preparing changes. For a task in a project that *uses* TopStitch, the [generator skill](.agents/skills/topstitch/SKILL.md) covers the caller's side of the API.

- For a connection or port export, start with `src/port_slice/`, `src/intf/`, and `tests/connections/` or `tests/interfaces/`. Check the instance connections and parent port declarations in the emitted Verilog.
- For a hierarchy or feedthrough change, look in `src/mod_def/` and `tests/modifications/` or `tests/feedthroughs/`. Check which module owns the added instance or ports. For pipelines, inspect the clock and stage parameters too.
- For placement and pinning, start with `src/mod_def/`, `src/lefdef.rs`, and `examples/pinning.rs`. Use the placement and pin tests in `tests/` to exercise coordinates, orientation, and bit order.
- For documentation, edit the chapters in `docs/guide/` and run the [guide checks](CONTRIBUTING.md#edit-and-preview-the-guide). The skill links to the same chapter sources.

## Keep this guidance useful

When you learn something about developing TopStitch that would have saved you time, update this file in the same change. Write the instruction you wish you had found, such as which test exercises a particular behavior. If an entry already covers the topic, improve it in place.

When a step in the user guide is unclear, check the answer against the code or a runnable example and fix the passage while the context is fresh. Follow the [documentation guidance](CONTRIBUTING.md#edit-and-preview-the-guide).
