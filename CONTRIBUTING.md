# Contributing to TopStitch

## Build and test a checkout

Follow the [installation guide](docs/guide/installation.md) to set up Rust and Slang. On Ubuntu, install `pkg-config`, `libssl-dev`, and `libcurl4-openssl-dev` for the checkout’s test dependencies. Run the test suite from the repository root:

```sh
cargo test --locked
```

Debug builds skip the performance tests. To measure them in release mode, run them one at a time and show their output:

```sh
cargo test --locked --release performance_ -- --test-threads=1 --show-output
```

For a faster edit cycle, run the test closest to the behavior you are changing. For example, this runs the interface export case that checks the emitted parent ports:

```sh
cargo test --locked --test test test_export_interface_with_prefix
```

Install [pre-commit](https://pre-commit.com/#install) and set up the hooks with `pre-commit install`. Run `pre-commit run --all-files` on your development branch to run the repository's formatting and file checks. CI runs the checks and the locked Cargo suite. [CI workflow](.github/workflows/ci.yml)

### Optional prebuilt Slang library

For the Ubuntu 24.04 x86_64 GNU toolchain and `slang-rs` 0.25.0, the release provides the native libraries. Run this block before your first Cargo build. The checksum is for this particular archive:

```sh
archive=$(mktemp)
prefix="$HOME/.local/share/slang-rs/0.25.0-ubuntu-24.04-x86_64-gnu"
curl -fL --retry 3 -o "$archive" https://github.com/xlsynth/slang-rs/releases/download/v0.25.0/slang-native-ubuntu-24.04-x86_64-gnu.tar.gz &&
  printf '%s  %s\n' 5f261b9f588523ab96cb9a4876dea83cf2d52ce435dbc8f226b4883ac7d8f62f "$archive" | sha256sum --check &&
  mkdir -p "$prefix" && tar -xzf "$archive" -C "$prefix" &&
  export SLANG_RS_NATIVE_DIR="$prefix" CC=gcc CXX=g++
```

Cargo still compiles the Rust bindings and C++ bridge. Repeat the `export` with your installation path in each new shell where you want to reuse the libraries. Use `unset SLANG_RS_NATIVE_DIR` to switch back to a source build. In an existing project, `cargo tree -i slang-rs` shows which release to match. [Slang release](https://github.com/xlsynth/slang-rs/releases/tag/v0.25.0)

## Report a bug or propose a change

A useful wiring bug report includes the TopStitch version, a small generator someone else can run, and the difference between expected and actual output. For example, show the two instance ports that should share a net and the emitted connections you observed. If the problem depends on imported RTL, a tiny module with just the necessary ports and parameters often makes a better reproduction than a full design.

Use examples and inputs you can publish. For a placement bug, construct the relevant outlines, tracks, and pins in the test. The [physical checks](tests/checks/lefdef.rs) illustrate this pattern, and the repository's [hooks](.pre-commit-config.yaml) exclude LEF/DEF files from commits.

In a pull request, describe what changes for callers and how you verified the result. Keep an example of the affected behavior alongside the appropriate tests. Update the guide when the change affects how someone would build a generator.

## Edit and preview the guide

Keep the README and user guide focused on what readers are trying to do. Explain when to choose a pattern, show it in use, and say what result to expect. Put signatures, parameter defaults, and exhaustive option lists in the Rust API docs. Link to the relevant type or method when readers need that detail.

Edit the Markdown in [docs/guide](docs/guide/SUMMARY.md). Its chapters also serve as the installable skill's offline references. Keep relative links between chapters and use public links for material outside the guide.

With [mdBook](https://rust-lang.github.io/mdBook/) installed, run `mdbook serve` at the repository root to preview changes. Use `mdbook build` to generate the site in `target/guide`. Chapters with runnable Rust snippets are included in the crate's doctests in `src/lib.rs`. To test the snippets and run both walkthroughs, install Icarus Verilog and run:

```sh
cargo test --locked --doc
cargo run --locked --example demo
(cd examples/tb && ./demo.sh)
cargo run --locked --example pinning
```

The [installation check](.github/workflows/installation.yml) runs the shell commands and Rust example from the installation chapter in a fresh Ubuntu 26.04 container, including building Slang from source.

The [guide workflow](.github/workflows/guide.yml) builds the book and offline skill on pull requests and publishes them to GitHub Pages on pushes to `main`. To enable hosting, select **GitHub Actions** as the repository’s Pages publishing source and allow `main` to deploy to the `github-pages` environment. Older guide snapshots are available as Markdown in Git history.
