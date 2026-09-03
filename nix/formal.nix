{ pkgs, fstar }:

pkgs.mkShell {
  packages = with pkgs; [
    fstar.packages.${pkgs.stdenv.hostPlatform.system}.fstar
    git
    just
    z3
  ];
  shellHook = ''
    unset LD_LIBRARY_PATH LD_PRELOAD NIX_LD NIX_LD_LIBRARY_PATH
    echo "Chitin F* verification shell"
    echo "Run: fstar.exe --help"
  '';
}
