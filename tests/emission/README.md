# Cross-emitter regression tests

Run `cargo test --locked --features emission-tests --test block_ir` with Python 3, Yosys, and Icarus Verilog (`iverilog` and `vvp`) on PATH. The all-features CI job runs this suite on every PR using the pinned nightly Rust toolchain. The test serializes and reparses the PIR package and uses the same SystemVerilog backend and default options as `xlsynth-driver block2sv`.

Add a structural `ModDef` fixture and call `check_design(&top)` in `parity.rs`. Put fixtures shared with the direct SV golden tests in `designs.rs`. The harness collects explicit net names from the generator. Yosys checks every top-level output for every two-state input combination. Before flattening, the harness also records port names, directions and shapes, instance names and types, and which bits of which pins share a wire. Port declaration order and automatically generated wire names can differ. In both files, a wire named on a whole instance port must match all its bits in order. A wire named on a slice must match exactly the width of that slice, with wire bit zero bound to the slice’s least significant port bit.

The cases cover sliced and bit-blasted assignments, constants wider than 64 bits, fanout, repeated child definitions, wrapped interfaces, two-way funnel wiring, and naming requests on drivers and receivers. The funnel also has an independently written behavioral reference. Negative controls change the selected data and swap physical names on an otherwise equivalent design. Existing direct SV golden tests cover formatting. The external-leaf tests link separately supplied SV with each emitter’s output. Yosys proves a nested combinational example against its specification, and Icarus simulates a clocked module with multiple parameter bindings and checks its instance paths. Rejection tests cover external roots, inout ports on native and external modules, and invalid or colliding names.

To compare two existing combinational SV designs with the same top-level interface:

```shell
python3 scripts/check_emission_equivalence.py gold.sv candidate.sv --top Top
```

Use `--structure` when both designs should expose the same structural hierarchy and pin connectivity. For example, `--net Top.framed_word=pack.word_o` also checks the requested wire bit for bit against that instance port. These checks operate on emitted RTL before synthesis. The comparison script checks combinational equivalence; the clocked fixture uses a directed simulation.
