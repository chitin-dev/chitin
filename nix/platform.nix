{ pkgs }:

with pkgs;
lib.optionals stdenv.hostPlatform.isLinux [
  alsa-lib
  fontconfig
  libX11
  libXi
  libXcursor
  libXrandr
  libxkbcommon
  libxkbcommon.dev
  vulkan-loader
  wayland
]
