{
  description = "Tsumugi: a cross-platform workspace for coding agents, terminals and Neovim";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";
    rust-overlay = {
      url = "git+https://github.com/oxalica/rust-overlay?shallow=1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    nix-github-release = {
      url = "github:lambdalisue/nix-github-release/v0.1.2";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      nix-github-release,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
      # Dioxus desktop draws through WebKitGTK on Linux; macOS uses the system webview.
      webviewLibraries =
        pkgs:
        pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.webkitgtk_4_1
          pkgs.gtk3
          pkgs.libsoup_3
          pkgs.glib
          pkgs.openssl
          pkgs.xdotool
        ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          inherit (pkgs) lib;
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain (_: rust);
          # The tag this flake was fetched at, from VERSION; see
          # github:lambdalisue/nix-github-release.
          version = nix-github-release.lib.version { inherit self; };
          build = {
            pname = "tsumugi";
            # Cargo.toml's placeholder, so the dependency layer stays cached
            # across releases; the package itself is stamped with the version.
            version = "0.0.0";
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./assets
                ./crates
                ./LICENSE
                ./THIRD-PARTY-LICENSES.txt
              ];
            };
            strictDeps = true;
            nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ];
            buildInputs =
              lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ] ++ webviewLibraries pkgs;
            # The tests run in CI and with `cargo xtask test`.
            doCheck = false;
          };
          tsumugi = craneLib.buildPackage (
            nix-github-release.lib.stampCargo { inherit pkgs version; } (
              build
              // {
                cargoArtifacts = craneLib.buildDepsOnly build;
                # Tsumugi's licence and those of the crates built into it go
                # with it.
                postInstall = ''
                  licenses="$out/share/licenses/tsumugi"
                  bash assets/release/install-notices.sh "$licenses"
                '';
                meta.mainProgram = "tsumugi";
              }
            )
          );
        in
        {
          inherit tsumugi;
          default = tsumugi;
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}
