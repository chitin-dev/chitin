#!/usr/bin/env bash
# Check the resolved Linux backend graph as well as compilation: Cargo features
# are additive, so a successful build alone does not prove that pruning worked.
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if [[ $(rustc -vV | sed -n 's/^host: //p') != *-linux-* ]]; then
  printf 'This check requires a Linux host.\n' >&2
  exit 1
fi

for variant in default wayland x11; do
  options=(-p chitin-desktop --locked)
  if [[ $variant != default ]]; then
    options+=(--no-default-features --features "$variant")
  fi

  printf 'Checking native backends: %s\n' "$variant"
  windows=$(cargo tree "${options[@]}" -e features -i winit --depth 1)
  graphics=$(cargo tree "${options[@]}" -e features -i wgpu --depth 1)

  if ! rg -q 'wgpu feature "vulkan"' <<< "$graphics"; then
    printf 'Linux build is missing Vulkan:\n%s\n' "$graphics" >&2
    exit 1
  fi
  if rg -q 'wgpu feature "(dx12|metal|web|webgpu)"' <<< "$graphics"; then
    printf 'Unexpected GPU backend features:\n%s\n' "$graphics" >&2
    exit 1
  fi

  for backend in wayland x11; do
    if [[ $variant == default || $variant == "$backend" ]]; then
      if ! rg -q "winit feature \"$backend\"" <<< "$windows"; then
        printf 'Missing %s window backend:\n%s\n' "$backend" "$windows" >&2
        exit 1
      fi
    elif rg -q "winit feature \"$backend\"" <<< "$windows"; then
      printf 'Unselected %s window backend is enabled:\n%s\n' "$backend" "$windows" >&2
      exit 1
    fi
  done

  if [[ $variant == wayland ]]; then
    dependencies=$(cargo tree "${options[@]}" -e normal --prefix none)
    if rg -q '^(device_query|x11) v' <<< "$dependencies"; then
      printf 'Wayland-only build still includes Xlib pointer polling.\n' >&2
      exit 1
    fi
  fi

  cargo check "${options[@]}"
done
