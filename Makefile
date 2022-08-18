all: test-contracts

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: test-contracts ## Run the contracts' tests
	@cargo test --manifest-path=./hatchery/Cargo.toml -- --nocapture

test-contracts: ## Build the test contracts
	@cargo build \
		--release \
		--manifest-path=modules/Cargo.toml \
		-Z build-std=core,alloc,panic_abort \
		-Z build-std-features=panic_immediate_abort \
		--target wasm32-unknown-unknown

.PHONY: all test-contracts
