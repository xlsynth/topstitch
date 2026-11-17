# topstitch

TopStitch is a Rust tool for top-level hardware integration: block connectivity,
floorplanning, and pin placement.

The online guide follows `main`. For an older snapshot, open `docs/guide/` at the tag or commit you use.

[User guide](https://xlsynth.github.io/topstitch/) · [Guide Markdown](docs/guide/SUMMARY.md) · [Rust API reference](https://docs.rs/topstitch) · [Agent skill](#use-with-a-coding-agent) · [Contributing](CONTRIBUTING.md)

## Installation

Install Rust using the [official Rust instructions](https://rust-lang.org/tools/install/). The [installation guide](docs/guide/installation.md) lists the Ubuntu build tools and walks through creating and running a project. Cargo builds Slang on first use. For faster builds on Ubuntu 24.04 x86-64, see the [optional prebuilt Slang library](CONTRIBUTING.md#optional-prebuilt-slang-library).

To start with the published crate, run `cargo add topstitch`. The dependency version for this checkout is:

```toml
[dependencies]
topstitch = "0.104.1"
```

## Basic usage

Define two module stubs, instantiate them in a top-level module, and connect
their ports:

```rust
use topstitch::{EmitOptions, IO, ModDef, Usage};

let a_def = ModDef::new("A");
a_def.add_port("data", IO::Output(8));
a_def.set_usage(Usage::EmitStubAndStop);

let b_def = ModDef::new("B");
b_def.add_port("data", IO::Input(8));
b_def.set_usage(Usage::EmitStubAndStop);

let top = ModDef::new("Top");
let a = top.instantiate(&a_def, Some("a"), None);
let b = top.instantiate(&b_def, Some("b"), None);
a.get_port("data").connect(&b.get_port("data"));

println!("{}", top.emit(EmitOptions::default()));
```

This emits stubs for `A` and `B` and an eight-bit wire connecting their instances in `Top`. For a runnable design with imported RTL, start with the [adder walkthrough](docs/guide/first-design.md). It takes you from a Rust generator to a Verilog simulation.

## Use with a coding agent

The [TopStitch skill](.agents/skills/topstitch/SKILL.md) helps an agent write a generator in a project that depends on the crate. From a checkout containing the guide, copy the skill and its reference chapters into your project's skill directory:

```sh
project=/path/to/your-project
mkdir -p "$project/.agents/skills"
cp -RL .agents/skills/topstitch "$project/.agents/skills/"
```

The `-L` option copies the guide chapters so the installed skill can read them offline. In Codex, invoke it with `$topstitch`, or ask a matching question such as “How do I expose this child's interface on its parent?” To use it across projects, install it in `~/.agents/skills/` instead.

The [downloadable skill](https://xlsynth.github.io/topstitch/topstitch-skill.tar.gz) follows the same commit on `main` as the online guide. Extract it into your agent’s skills directory. Its `REVISION` file records the commit.
