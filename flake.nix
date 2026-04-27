{
  description = "claude-usage-optimization: Rust workspace for ingesting and querying Claude Code transcripts";

  inputs = {
    # Pinned to nixos-unstable so we get a recent rustc available via rust-overlay.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    systems.url = "github:nix-systems/default";

    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";

    devshell.url = "github:numtide/devshell";
    devshell.inputs.nixpkgs.follows = "nixpkgs";

    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";

    # rust-overlay reads rust-toolchain.toml so the pinned channel + components
    # are the single source of truth for the whole repo (cargo, clippy, rustfmt).
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      flake-parts,
      systems,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import systems;

      imports = [
        inputs.devshell.flakeModule
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        { pkgs, system, ... }:
        let
          # Single source of truth for the Rust toolchain — pinned in
          # rust-toolchain.toml at the repo root.
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

          # Garnix Action bodies live in ./nix/garnix.nix. Pulling them here
          # exposes flake.apps.<system>.<name>; garnix.yaml then references
          # them by name in its `actions:` block.
          garnix = import ./nix/garnix.nix { inherit pkgs rustToolchain; };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          inherit (garnix) apps;

          treefmt = {
            projectRootFile = "flake.nix";
            # Start with nixfmt only so this PR doesn't reformat the existing
            # Rust / TS / Markdown tree. Add per-language formatters in their
            # own PRs (rustfmt via rustToolchain, taplo, shfmt, prettier) so
            # each formatting sweep is reviewable on its own.
            programs.nixfmt.enable = true;
            settings.global.excludes = [
              "*.lock"
              "LICENSE-*"
              "target/**"
              "crates/*/web/dist/**"
              "crates/*/web/node_modules/**"
            ];
          };

          devshells.default = {
            name = "claude-usage-optimization";

            motd = ''
              {bold}claude-usage-optimization{reset} dev shell — run {bold}menu{reset} for commands.
            '';

            packages = [
              rustToolchain
              # cargo-release drives the version bump + tag flow described in
              # the project README's Release section.
              pkgs.cargo-release
              pkgs.cargo-nextest
              # duckdb (bundled) compiles a C++ source tree from build.rs.
              pkgs.cmake
              pkgs.pkg-config
              # The duckdb CLI is what the agent skills shell out to — required
              # alongside cct itself per the project README.
              pkgs.duckdb
              # build.rs in claude-code-transcripts-ingest invokes `npm` to
              # build the embedded React viewer.
              pkgs.nodejs_22
            ];

            commands = [
              {
                category = "build";
                name = "build";
                help = "cargo build the whole workspace";
                command = "cargo build --workspace \"$@\"";
              }
              {
                category = "build";
                name = "release";
                help = "cargo build --release the whole workspace";
                command = "cargo build --workspace --release \"$@\"";
              }
              {
                category = "check";
                name = "clippy";
                help = "cargo clippy across the workspace, all targets, deny warnings";
                command = "cargo clippy --workspace --all-targets -- -D warnings \"$@\"";
              }
              {
                category = "check";
                name = "test";
                help = "cargo test the whole workspace";
                command = "cargo test --workspace \"$@\"";
              }
              {
                category = "format";
                name = "fmt";
                help = "run treefmt across every supported file in the repo (nix fmt)";
                command = "nix fmt \"$@\"";
              }
              {
                category = "run";
                name = "cct";
                help = "cct CLI passthrough — try cct-ingest / cct-serve / cct-info, or `cct --help`";
                command = "cargo run --quiet --release -p claude-code-transcripts-ingest --bin cct -- \"$@\"";
              }
              {
                category = "run";
                name = "cct-ingest";
                help = "ingest ~/.claude/projects into ~/.local/share/cct/transcripts.duckdb";
                command = "cargo run --quiet --release -p claude-code-transcripts-ingest --bin cct -- ingest \"$@\"";
              }
              {
                category = "run";
                name = "cct-serve";
                help = "serve the embedded transcript viewer at http://localhost:8766";
                command = "cargo run --quiet --release -p claude-code-transcripts-ingest --bin cct -- serve \"$@\"";
              }
              {
                category = "run";
                name = "cct-info";
                help = "print DB path, size, entry / session counts, last ingest timestamp";
                command = "cargo run --quiet --release -p claude-code-transcripts-ingest --bin cct -- info \"$@\"";
              }
              {
                category = "update";
                name = "update-flake";
                help = "refresh flake.lock to the latest pinned input revisions";
                command = "nix flake update \"$@\"";
              }
            ];
          };
        };
    };
}
