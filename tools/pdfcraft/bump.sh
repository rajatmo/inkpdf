#!/bin/bash
# Move the pinned pdfcraft to another commit: every `rev` of a pdfcraft line in the root Cargo.toml (the pdfcraft-* crates and the vendored
# hayro*, lopdf and vello_common lines, which must all agree) is set to REV, the lock file is brought along, and the PDF backend
# conformance suite (apps/inkpdf/src/pdf/contract.rs) and the whole test suite run. What changed is printed at the end.
# The rule (docs/PDF-BACKEND.md): bump only with the suite green, in a commit of its own, with the numbers of `INKPDF_MEASURE=1` beside it.
#   usage: tools/pdfcraft/bump.sh <40-hex commit of storytold/pdfcraft>
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
rev="${1:-}"
[[ "$rev" =~ ^[0-9a-f]{40}$ ]] || { echo "usage: $0 <40-hex commit of storytold/pdfcraft>"; exit 2; }
cd "$root"
before="$(grep -c "storytold/pdfcraft\", rev = \"$rev\"" Cargo.toml || true)"
total="$(grep -c 'storytold/pdfcraft"' Cargo.toml)"
# Only the pdfcraft lines: the winit fork is another repository with a rev of its own.
sed -i -E "/storytold\/pdfcraft\"/ s/rev = \"[0-9a-f]{40}\"/rev = \"$rev\"/" Cargo.toml
after="$(grep -c "storytold/pdfcraft\", rev = \"$rev\"" Cargo.toml || true)"
[ "$after" = "$total" ] || { echo "only $after of $total pdfcraft lines are at $rev: look at Cargo.toml"; exit 1; }
if [ "$before" = "$total" ]; then
    echo "already at $rev ($total lines): nothing to change, running the suite anyway"
else
    echo "moved $((total - before)) of $total lines to $rev"
    cargo update -p pdfcraft-cos
fi
cargo test -p inkpdf --lib pdf::contract
cargo test -p inkpdf --lib
echo
git --no-pager diff --stat -- Cargo.toml Cargo.lock | cat
echo "suite green at $rev"
