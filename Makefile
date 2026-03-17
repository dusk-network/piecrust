help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

COMPILER_VERSION=v0.3.0-rc

setup-compiler: ## Setup the dusk compiler
	@./scripts/setup-compiler.sh $(COMPILER_VERSION)

contracts: ## Build example contracts
	@RUSTFLAGS="-C link-args=-zstack-size=65536" \
	cargo build \
	  --release \
	  --manifest-path=contracts/Cargo.toml \
	  --color=always \
	  --target wasm32-unknown-unknown
	@mkdir -p target/stripped
	@find target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" \
	    | xargs -I % basename % \
	    | xargs -I % ./scripts/strip.sh \
	 	          target/wasm32-unknown-unknown/release/% \
	 	          target/stripped/%
	@$(MAKE) contracts-wasm64

contracts-wasm64: setup-compiler ## Build wasm64 contracts
	@cargo +dusk build \
	  --release \
	  --manifest-path=contracts/c-example/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc \
	  --target wasm64-unknown-unknown
	@mkdir -p target/stripped
	@./scripts/strip.sh \
	  target/wasm64-unknown-unknown/release/c_example.wasm \
	  target/stripped/c_example.wasm

test: contracts cold-reboot assert-counter-contract-small ## Run all tests
	@$(MAKE) -C ./crumbles $@
	@$(MAKE) -C ./piecrust-uplink $@
	@$(MAKE) -C ./piecrust $@

clippy: ## Run clippy on all crates
	@$(MAKE) -C ./crumbles $@
	@$(MAKE) -C ./piecrust-uplink $@
	@$(MAKE) -C ./piecrust $@

no-std: ## Run no_std build check
	@$(MAKE) -C ./piecrust-uplink $@

fmt: ## Format all code
	@cargo +nightly fmt --all

check: ## Run cargo check
	@cargo check

doc: ## Build documentation
	@cargo doc --no-deps

clean: ## Clean build artifacts
	@cargo clean

cold-reboot: contracts ## Run the cold reboot test
	@cargo build \
	  --manifest-path=./piecrust/tests/cold-reboot/Cargo.toml \
	  --color=always
	@rm -rf /tmp/piecrust-cold-reboot
	@./target/debug/cold_reboot /tmp/piecrust-cold-reboot initialize
	@./target/debug/cold_reboot /tmp/piecrust-cold-reboot confirm
	@rm -r /tmp/piecrust-cold-reboot

MAX_COUNTER_CONTRACT_SIZE = 8192

assert-counter-contract-small: contracts
	@test `wc -c target/stripped/counter.wasm | sed 's/^[^0-9]*\([0-9]*\).*/\1/'` -lt $(MAX_COUNTER_CONTRACT_SIZE);

.PHONY: help test contracts contracts-wasm64 setup-compiler cold-reboot assert-counter-contract-small clippy no-std fmt check doc clean
