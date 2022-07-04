all: ## Generate the optimized WASM for the contract given
	@cargo build \
          --release \
          --manifest-path=./Cargo.toml \
          -Z build-std=std,panic_abort \
          -Z build-std-features=panic_immediate_abort \
          --target wasm32-unknown-unknown \

