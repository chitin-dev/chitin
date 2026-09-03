{ pkgs, platform }:

pkgs.mkShell {
  packages = with pkgs; [
    cargo
    clippy
    git
    just
    mdbook
    mdbook-katex
    pkg-config
    rustc
    rustfmt
    typst
    z3
  ];
  buildInputs = platform;
  shellHook = ''
    unset LD_LIBRARY_PATH LD_PRELOAD NIX_LD NIX_LD_LIBRARY_PATH
    echo "Chitin development shell"
    echo "Run: just ci"
  '';
}
