#!/usr/bin/env bash
# Start a new extension: copies apps/inkpdf-ext/template to apps/inkpdf-ext/src/<name>/ with the name and label filled in.
#   tools/new-extension.sh <name> "<Label>"        (name: lower case letters, digits and '-', beginning with a letter)
# Nothing else is edited: build.rs finds the folder. See docs/EXTENSIONS.md.
set -euo pipefail
name="${1:?usage: tools/new-extension.sh <name> \"<Label>\"}"
label="${2:?usage: tools/new-extension.sh <name> \"<Label>\"}"
[[ "$name" =~ ^[a-z][a-z0-9-]{0,23}$ ]] || { echo "the name must be lower case letters, digits and '-', beginning with a letter, 24 at most"; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/apps/inkpdf-ext/src/${name//-/_}"
[ -e "$dir" ] && { echo "$dir exists"; exit 1; }
mkdir -p "$dir"
for f in mod.rs icon.svg; do
    sed -e "s/TEMPLATE_ID/$name/g" -e "s/TEMPLATE_LABEL/${label//\//\\/}/g" "$root/apps/inkpdf-ext/template/$f" > "$dir/$f"
done
echo "made $dir"
echo "next: edit mod.rs and icon.svg, then  cargo test -p inkpdf-ext -p inkpdf --lib  (the contract suite runs on your tool by itself)"
