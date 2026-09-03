{ pkgs }:

pkgs.mkShell {
  packages = with pkgs; [
    git
    just
    mdbook
    mdbook-katex
    typst
  ];
  shellHook = ''
    unset LD_LIBRARY_PATH LD_PRELOAD NIX_LD NIX_LD_LIBRARY_PATH
    echo "Chitin documentation shell"
    echo "Run: just docs-build"
  '';
}
