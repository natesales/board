{ pkgs ? import <nixpkgs> {} }:

# Use cargo-zigbuild for simpler cross-compilation (no Docker needed)
pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.rustup
    pkgs.zig
    pkgs.cargo-zigbuild
    pkgs.cmake
    pkgs.perl
    pkgs.llvmPackages.libclang
    pkgs.llvmPackages.clang
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.fontconfig
    pkgs.pkg-config
  ];

  # Disable jitterentropy in aws-lc-sys (Zig doesn't support -U_FORTIFY_SOURCE)
  AWS_LC_SYS_NO_JITTER_ENTROPY = "1";

  # For bindgen to find libclang
  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

  # For winit/wayland to find libraries at runtime
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.fontconfig
  ];

  shellHook = ''
    # Ensure rustup has a default toolchain
    if ! rustup show active-toolchain &>/dev/null; then
      echo "Installing stable Rust toolchain..."
      rustup default stable
    fi

    # Add the ARM target if not already present
    if ! rustup target list --installed | grep -q armv7-unknown-linux-gnueabihf; then
      echo "Adding armv7-unknown-linux-gnueabihf target..."
      rustup target add armv7-unknown-linux-gnueabihf
    fi

    echo "Cross-compilation environment ready!"
    echo "Build for ARM:     cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.28 --no-default-features --features framebuffer"
    echo "Build for desktop: cargo run"
  '';
}
