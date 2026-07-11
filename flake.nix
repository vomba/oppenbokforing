{
  description = "ÖppenBokföring — Tauri v2 + React + Rust dev environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      mkShellFor = system:
        let
          pkgs = import nixpkgs { inherit system; };
          lib = pkgs.lib;
        in
        pkgs.mkShell {
          name = "oppenbokforing-desktop";
          packages = with pkgs;
            [
              nodejs_22
              rustc
              cargo
              rustfmt
              clippy
              sqlx-cli
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              pkg-config
              openssl
              webkitgtk_4_1
              gtk3
              librsvg
              gdk-pixbuf
              libappindicator
            ];
          shellHook = ''
            echo "ÖppenBokföring dev shell"
            echo "  node:  $(node --version)"
            echo "  npm:   $(npm --version)"
            echo "  rustc: $(rustc --version)"
            echo "  cargo: $(cargo --version)"
            if [ ! -d node_modules ]; then
              echo "  hint: run npm install"
            fi
            ${lib.optionalString pkgs.stdenv.isDarwin ''
              if ! xcode-select -p >/dev/null 2>&1; then
                echo "  warn: install Xcode Command Line Tools for tauri:build (xcode-select --install)"
              fi
            ''}
          '';
        };
    in
    {
      devShells = builtins.listToAttrs (
        map (system: {
          name = system;
          value = { default = mkShellFor system; };
        }) systems
      );
    };
}
