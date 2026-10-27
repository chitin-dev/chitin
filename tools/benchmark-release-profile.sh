#!/usr/bin/env bash
# Both profiles must run on the same idle machine with a fresh CRITERION_HOME.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if [[ $# != 2 || -z ${CRITERION_HOME:-} ]]; then
  printf 'Usage: CRITERION_HOME=DIR bash tools/benchmark-release-profile.sh PROFILE LABEL\n' >&2
  exit 1
fi

profile=$1
label=$2
options=(--locked --profile "$profile")
measurements=(--save-baseline "$label" --sample-size 30 --warm-up-time 0.5 --measurement-time 2 --noplot)

cargo bench -p chitin-desktop --bench sequence_virtualization "${options[@]}" -- "${measurements[@]}"
cargo bench -p chitin-ui --bench tree_virtualization "${options[@]}" -- "${measurements[@]}"
cargo bench -p chitin-bio --bench molecular_surface "${options[@]}" -- "${measurements[@]}"
cargo bench -p chitin-molecule-renderer --bench native_frame "${options[@]}" -- "${measurements[@]}"
