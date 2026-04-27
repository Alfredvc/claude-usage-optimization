{ pkgs, rustToolchain }:
let
  # Garnix substituter creds — keeps any nested `nix` calls cache-warm.
  setupEnv = ''
    export HOME=$(mktemp -d)
    export CARGO_HOME="$HOME/.cargo"
    cd "$PWD"

    export NIX_CONFIG="experimental-features = nix-command flakes
    accept-flake-config = true
    extra-substituters = https://cache.garnix.io
    extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g="
  '';

  # Toolchain + build dependencies the workspace needs:
  #   - rustToolchain    pinned via rust-toolchain.toml (cargo, rustc, rustfmt, clippy)
  #   - cmake / gcc      duckdb-bundled compiles a vendored C++ tree
  #   - pkg-config       build.rs probes for native libs
  #   - nodejs_22        build.rs runs `npm ci && npm run build` for the embedded viewer
  #   - git / coreutils  generic shell hygiene
  toolchainPath = pkgs.lib.makeBinPath [
    rustToolchain
    pkgs.cmake
    pkgs.coreutils
    pkgs.gcc
    pkgs.git
    pkgs.nodejs_22
    pkgs.pkg-config
  ];

  cargoTestScript = pkgs.writeShellScript "cargo-test" ''
    set -uo pipefail
    trap 'exit $?' EXIT

    ${setupEnv}
    export PATH="${toolchainPath}:$PATH"

    # Garnix Action runners cap at ~4.5 GB of memory (not in official docs;
    # see [notes] in the garnix-ci skill). duckdb-bundled compiles a large
    # C++ tree, and each parallel cc1plus instance peaks around 1.5–2 GB,
    # so the default job count overruns the ceiling and the OOM killer
    # reaps cc1plus mid-compile. Serialising both layers (cargo's job pool
    # and cc-rs's NUM_JOBS) keeps peak memory to a single cc1plus process,
    # at the cost of slower compilation.
    export CARGO_BUILD_JOBS=1
    export NUM_JOBS=1

    cargo test --workspace --locked
  '';

in
{
  apps.cargo-test = {
    type = "app";
    program = toString cargoTestScript;
  };
}
