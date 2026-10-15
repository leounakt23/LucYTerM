{
  description = "Remote App secure native remote computing client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        package = pkgs.rustPlatform.buildRustPackage {
          pname = "remote-app";
          version = "1.0.0";
          src = ../..;
          cargoLock.lockFile = ../../Cargo.lock;
          # Feature-set note (distribution matrix): default cargo features
          # (ssh,telnet), matching packaging/debian/rules (DEB proven).
          # The Flatpak recipe is the intentional `--all-features`
          # exception. buildRustPackage installs all cargo binaries, so
          # both remote-app and remote-app-headless land in $out/bin.
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.dbus pkgs.libudev-zero ];
          postInstall = ''
            install -Dm644 assets/remote-app.desktop $out/share/applications/remote-app.desktop
            install -Dm644 assets/remote-app.metainfo.xml $out/share/metainfo/com.github.LucYTerM.remote-app.metainfo.xml
            install -Dm644 assets/icon.svg $out/share/icons/hicolor/scalable/apps/remote-app.svg
            install -Dm644 docs/remote-app.1 $out/share/man/man1/remote-app.1
          '';
          meta.mainProgram = "remote-app";
        };
      in {
        packages.default = package;
        apps.default = flake-utils.lib.mkApp { drv = package; };
        overlays.default = final: prev: { remote-app = package; };
      });
}
