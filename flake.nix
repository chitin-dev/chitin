{
  description = "Reproducible development environments for Chitin";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fstar.url = "github:FStarLang/FStar";
  };

  outputs = { self, nixpkgs, fstar }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forEachSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
          platform = import ./nix/platform.nix { inherit pkgs; };
        in {
          default = import ./nix/devshell.nix { inherit pkgs platform; };
          docs = import ./nix/docs.nix { inherit pkgs; };
          formal = import ./nix/formal.nix { inherit pkgs fstar; };
        });
    };
}
