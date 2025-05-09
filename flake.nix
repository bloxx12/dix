{
  description = "Diff Nix";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs = inputs: let
    eachSystem = inputs.nixpkgs.lib.genAttrs (import inputs.systems);
    pkgsFor = inputs.nixpkgs.legacyPackages;
  in {
    packages = eachSystem (system: {
      default = inputs.self.packages.${system}.ralc;
      ralc = pkgsFor.${system}.callPackage ./nix/package.nix {};
    });

    devShells = eachSystem (system: {
      default = pkgsFor.${system}.mkShell {
        packages = builtins.attrValues {
          inherit
            (pkgsFor.${system})
            cargo
            rustc
            rustfmt
            bacon
            ;
          inherit
            (pkgsFor.${system}.rustPackages)
            clippy
            ;
        };
      };
    });
  };
}
