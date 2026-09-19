{ pkgs }:

pkgs.mkShell {
  packages = with pkgs; [
    git
    just
    cargo
    rustc
    typst
  ];
  shellHook = ''
    unset LD_LIBRARY_PATH LD_PRELOAD NIX_LD NIX_LD_LIBRARY_PATH
    echo "Chitin documentation shell"
    echo "Install Shiroa 0.4.0 with: cargo install shiroa --version 0.4.0 --locked"
    echo "Then run: just docs-build"
  '';
}
