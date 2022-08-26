all: test-modules

COUNTER_MODULE_BYTE_SIZE_LIMIT = 4000

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: release-modules test-modules assert-counter-module-small ## Run the modules' tests
	@cargo test --manifest-path=./hatchery/Cargo.toml

test-modules: ## Build the test modules
	@cargo build \
		--color=always \
		--features=dallo/debug \
		--manifest-path=modules/Cargo.toml \
		--target wasm32-unknown-unknown

release-modules: ## Build the test modules in release mode
	@cargo build \
		--color=always \
		--release \
		--manifest-path=modules/Cargo.toml \
		-Z build-std=core,alloc,panic_abort \
		-Z build-std-features=panic_immediate_abort \
		--target wasm32-unknown-unknown \

assert-counter-module-small:
	@test `wc -c ./modules/target/wasm32-unknown-unknown/release/counter.wasm | cut -f1 -d' '` -lt $(COUNTER_MODULE_BYTE_SIZE_LIMIT);

.PHONY: all test-modules
