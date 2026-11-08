{
  description = "Rust development environment for Abyss";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
    in {
      devShells = nixpkgs.lib.genAttrs systems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer pkg-config ];
            buildInputs = with pkgs; [ alsa-lib ];
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [ alsa-lib ]);
            shellHook = ''
              export CARGO_HOME="$PWD/.cargo-home"
            '';
          };
        });
    };
}
