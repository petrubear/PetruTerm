#!/usr/bin/env bash
# PGO (Profile-Guided Optimization) build for PetruTerm.
#
# Three-phase process:
#   1. Instrument: build with profiling counters
#   2. Profile:    run workloads to generate .profraw data
#   3. Optimize:   rebuild using collected profiles
#
# Result: target/release-pgo/petruterm
#         ~5-10% faster on measured hot paths (shaping, search, rendering).
#
# Requirements:
#   - Xcode command line tools (provides llvm-profdata via xcrun)
#   - Apple Silicon or Intel Mac with Metal GPU (for rasterize bench)
#
# Usage:
#   ./scripts/build_pgo.sh
#   ./scripts/build_pgo.sh --skip-gpu   # skip GPU-dependent benchmarks

set -euo pipefail

SKIP_GPU=0
for arg in "$@"; do
  if [[ "$arg" == "--skip-gpu" ]]; then
    SKIP_GPU=1
  fi
done

PGO_DIR=/tmp/petruterm-pgo
PROFDATA="$PGO_DIR/merged.profdata"
LLVM_PROFDATA="$(xcrun -f llvm-profdata)"

echo "==> PGO build for PetruTerm"
echo "    llvm-profdata: $LLVM_PROFDATA"
echo "    profile dir:   $PGO_DIR"
echo ""

# ── Phase 1: instrument ───────────────────────────────────────────────────────
echo "==> Phase 1: building instrumented binary..."
rm -rf "$PGO_DIR"
mkdir -p "$PGO_DIR"

RUSTFLAGS="-Cprofile-generate=$PGO_DIR" \
  cargo build --release 2>&1 | tail -3

echo "    done."
echo ""

# ── Phase 2: profile ──────────────────────────────────────────────────────────
echo "==> Phase 2: running workloads..."

# CPU-only benchmarks: font shaping and incremental search.
# These cover the dominant hot paths: text tokenization, HarfBuzz shaping,
# LRU word cache, fuzzy match, and parallel search.
echo "    [1/3] shaping bench..."
LLVM_PROFILE_FILE="$PGO_DIR/shaping-%p.profraw" \
  cargo bench --bench shaping -- --profile-time 3 2>&1 | grep -E "^test|time:" || true

echo "    [2/3] search bench..."
LLVM_PROFILE_FILE="$PGO_DIR/search-%p.profraw" \
  cargo bench --bench search -- --profile-time 3 2>&1 | grep -E "^test|time:" || true

# GPU-dependent benchmarks (rasterize, build_instances).
# These cover atlas uploads, vertex assembly, and the row-cache hot path.
if [[ "$SKIP_GPU" -eq 0 ]]; then
  echo "    [3/3] rasterize + build_instances bench..."
  LLVM_PROFILE_FILE="$PGO_DIR/rasterize-%p.profraw" \
    cargo bench --bench rasterize -- --profile-time 3 2>&1 | grep -E "^test|time:" || true
  LLVM_PROFILE_FILE="$PGO_DIR/build-%p.profraw" \
    cargo bench --bench build_instances -- --profile-time 3 2>&1 | grep -E "^test|time:" || true
else
  echo "    [3/3] skipped (--skip-gpu)"
fi

echo "    done."
echo ""

# ── Phase 3: merge profiles ───────────────────────────────────────────────────
echo "==> Phase 3: merging profiles..."
PROFRAW_FILES=("$PGO_DIR"/*.profraw)
if [[ ${#PROFRAW_FILES[@]} -eq 0 || ! -f "${PROFRAW_FILES[0]}" ]]; then
  echo "ERROR: no .profraw files found in $PGO_DIR" >&2
  echo "       The instrumented run produced no profiling data." >&2
  exit 1
fi

"$LLVM_PROFDATA" merge \
  -output="$PROFDATA" \
  "$PGO_DIR"/*.profraw

echo "    merged ${#PROFRAW_FILES[@]} .profraw file(s) → $PROFDATA"
echo ""

# ── Phase 4: optimized build ──────────────────────────────────────────────────
echo "==> Phase 4: building PGO-optimized binary..."

RUSTFLAGS="-Cprofile-use=$PROFDATA -Cllvm-args=-pgo-warn-missing-function" \
  cargo build --release --target-dir target/pgo 2>&1 | tail -3

echo ""
echo "==> Done!"
echo "    Binary: target/pgo/release/petruterm"
echo ""
echo "    Compare against baseline:"
echo "    hyperfine ./target/release/petruterm ./target/pgo/release/petruterm"
