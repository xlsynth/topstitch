#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Run inside a fresh Ubuntu container with the guide mounted at /installation.md.
set -euo pipefail

# Extract one fenced example from the named Markdown section. Fail if it disappears.
snippet() {
  awk -v section="## $1" -v lang="$2" '
    /^## / { selected = ($0 == section) }
    selected && $0 == ("```" lang) { copying = 1; next }
    copying && $0 == "```" { closed = 1; exit }
    copying { print }
    END { if (!closed) exit 1 }
  ' /installation.md
}

snippet 'Install the build tools' sh | bash -eu

# Automate the guide's manual Rust install using the official rustup binary.
cd /tmp
rustup_url=https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
curl -fsSLO --retry 3 "$rustup_url"
curl -fsSL --retry 3 "$rustup_url.sha256" | sha256sum --check --strict
chmod 755 rustup-init
runuser -u ubuntu -- ./rustup-init -y --profile minimal --default-toolchain stable

{
  echo 'cd "$HOME"'
  echo 'source "$HOME/.cargo/env"'
  snippet 'Install Rust' sh
  snippet 'Create a project' sh
  echo "cat > src/main.rs <<'RUST'"
  snippet 'Create a project' rust
  echo 'RUST'
  snippet 'Run the generator' sh
} > /tmp/run-generator.sh

runuser -u ubuntu -- bash -euo pipefail /tmp/run-generator.sh | tee /tmp/generator-output
grep -Fq 'module Panel(' /tmp/generator-output
grep -Fq 'Led led (' /tmp/generator-output
grep -Fq '.rgb(rgb)' /tmp/generator-output
