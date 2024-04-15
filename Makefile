help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

COMPILER_VERSION=v0.2.0

setup-compiler: ## Setup the Dusk Contract Compiler
	@./scripts/setup-compiler.sh $(COMPILER_VERSION)

contracts: contracts_32 contracts_64 ## Build all example contracts
	@cp -r target/stripped_64 target/stripped

contracts_32: setup-compiler ## Build 32-bit example contracts
	@RUSTFLAGS="-C link-args=-zstack-size=65536" \
	cargo +dusk build \
	  --release \
	  --manifest-path=contracts/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc \
	  --target wasm32-unknown-unknown
	@mkdir -p target/stripped_32
	@find target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" \
	    | xargs -I % basename % \
	    | xargs -I % ./scripts/strip.sh \
	 	          target/wasm32-unknown-unknown/release/% \
	 	          target/stripped_32/%

contracts_64: setup-compiler ## Build 64-bit example contracts
	@RUSTFLAGS="-C link-args=-zstack-size=65536" \
	cargo +dusk build \
	  --release \
	  --manifest-path=contracts/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc \
	  --target wasm64-unknown-unknown
	@mkdir -p target/stripped_64
	@find target/wasm64-unknown-unknown/release -maxdepth 1 -name "*.wasm" \
	    | xargs -I % basename % \
	    | xargs -I % ./scripts/strip.sh \
	 	          target/wasm64-unknown-unknown/release/% \
	 	          target/stripped_64/%

test: contracts cold-reboot assert-counter-contract-small ## Run all tests
	@cargo test \
	  --manifest-path=./crumbles/Cargo.toml \
	  --all-features \
	  --color=always
	@cargo test \
	  --manifest-path=./piecrust/Cargo.toml \
	  --all-features \
	  --color=always

cold-reboot: contracts ## Run the cold reboot test
	@cargo build \
	  --manifest-path=./piecrust/tests/cold-reboot/Cargo.toml \
	  --color=always
	@rm -rf /tmp/piecrust-cold-reboot
	@./target/debug/cold_reboot /tmp/piecrust-cold-reboot initialize
	@./target/debug/cold_reboot /tmp/piecrust-cold-reboot confirm
	@rm -r /tmp/piecrust-cold-reboot

.PHONY: test contracts contracts_64 contracts_32 cold-reboot assert-counter-contract-small

MAX_COUNTER_CONTRACT_SIZE = 8192

assert-counter-contract-small: contracts
	@test `wc -c target/stripped/counter.wasm | sed 's/^[^0-9]*\([0-9]*\).*/\1/'` -lt $(MAX_COUNTER_CONTRACT_SIZE);
