#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🦀 Building Rust backend with egui..."

# Change to project root
cd "$PROJECT_ROOT"

# Clean previous builds
cargo clean

# Build optimized release binary
cargo build --release

echo "✅ Rust backend built successfully"
echo "✨ GOATd Kernel ready!"
echo "Binary location: $PROJECT_ROOT/target/release/goatd_kernel"
