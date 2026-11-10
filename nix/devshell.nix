{ pkgs, platform }:

pkgs.mkShell {
  packages = with pkgs; [
    cargo
    clippy
    git
    just
    nodejs
    pnpm
    pkg-config
    rustc
    rustfmt
    typst
    wasm-pack
    zenity
    z3
  ];
  buildInputs = platform;
  shellHook = ''
    unset LD_LIBRARY_PATH LD_PRELOAD NIX_LD NIX_LD_LIBRARY_PATH
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath platform}"
    echo "Chitin development shell"
    echo "Run: just ci"
  '';
}
