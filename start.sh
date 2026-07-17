#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")"

echo "Generating README files..."
cargo run --release

echo
echo "README.md and README_RU.md have been updated."