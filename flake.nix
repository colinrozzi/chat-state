{
  description = "chat-state - A Theater actor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    
    # Add cargo-component source
    cargo-component-src = {
      url = "github:bytecodealliance/cargo-component/v0.21.1";
      flake = false;
    };
  };


  outputs = { self, nixpkgs, rust-overlay, flake-utils, cargo-component-src, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" "wasm32-wasip1" ];
        };
        
        # Build cargo-component
        cargo-component = pkgs.rustPlatform.buildRustPackage {
          pname = "cargo-component";
          version = "0.21.1";
          src = cargo-component-src;
          
          cargoLock = {
            lockFile = pkgs.runCommand "cargo-component-Cargo.lock" {} ''
              cp ${cargo-component-src}/Cargo.lock $out
            '';
          };
          
          buildInputs = with pkgs; [
            openssl
            pkg-config
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
          
          # Skip tests during build
          doCheck = false;
        };

      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            # Pre-built cargo-component
            cargo-component
            # Tools for WebAssembly development
            wasmtime
            binaryen
            wasm-tools
            # Development tools
            rustfmt
            clippy
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          # Set SSL certificates path
          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
        };

        packages.default = pkgs.stdenv.mkDerivation {
          pname = "chat-state";
          version = "0.1.0";
          src = ./.;

          nativeBuildInputs = with pkgs; [ 
            rustToolchain
            pkg-config 
            wasm-tools
            binaryen
            cargo-component
            cacert
            rustup
          ];
          
          buildInputs = with pkgs; [ 
            openssl
          ];

          buildPhase = ''
            # Create cache directories
            export CARGO_HOME=$TMPDIR/cargo
            export XDG_CACHE_HOME=$TMPDIR/cache
            export CARGO_COMPONENT_CACHE_DIR=$TMPDIR/cargo-component-cache
            mkdir -p $CARGO_HOME $XDG_CACHE_HOME $CARGO_COMPONENT_CACHE_DIR
            
            # Ensure SSL certificates are available
            export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
            export NIX_SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
            
            # Build the WebAssembly component
            cargo component build --release --target wasm32-unknown-unknown
          '';

          installPhase = ''
            mkdir -p $out/lib

            wasmFile=$(ls target/wasm32-unknown-unknown/release/*.wasm)
            if [ -z "$wasmFile" ]; then
              echo "Error: No .wasm file found in target/wasm32-unknown-unknown/release/"
              exit 1
            fi
            # Ensure there is only one .wasm file
            if [ $(echo $wasmFile | wc -w) -ne 1 ]; then
              echo "Error: More than one .wasm file found in target/wasm32-unknown-unknown/release/"
              exit 1
            fi
            cp $wasmFile $out/lib/component.wasm
          '';
          
          # No longer need network access during build
          __noChroot = false;
        };
      });
}
