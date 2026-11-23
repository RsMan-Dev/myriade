# The server build of the game (`horizon-game --no-default-features`, its `horizon-client` binary): the
# dedicated server, run headless by horizon-server as `horizon-engine` (docs/hosting.md).
{
  lib,
  rustPlatform,
  pkg-config,
  callPackage,
  version,
}:
rustPlatform.buildRustPackage {
  pname = "horizon-engine";
  inherit version;
  src = callPackage ./source.nix { };
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "--package=horizon-game"
    "--bin=horizon-client"
  ];
  buildNoDefaultFeatures = true;

  nativeBuildInputs = [ pkg-config ];

  # The game's tests read the prepared vanilla files in .local/, which a build never has.
  doCheck = false;

  postInstall = ''
    mv $out/bin/horizon-client $out/bin/horizon-engine
  '';

  meta = {
    description = "Horizon's headless game engine, run by horizon-server";
    homepage = "https://playhorizon.dev";
    license = lib.licenses.lgpl3Plus;
    mainProgram = "horizon-engine";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
