help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

modules: ## Build WASM modules
	@cargo build \
	  --release \
      --manifest-path=modules/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc,panic_abort \
	  -Z build-std-features=panic_immediate_abort \
	  --target wasm32-unknown-unknown
	@mkdir -p modules/target/stripped
	@find modules/target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" \
	 | xargs -I % basename % \
	 | xargs -I % wasm-tools strip -a \
	 	          modules/target/wasm32-unknown-unknown/release/% \
	 	          -o modules/target/stripped/%

test: modules assert-counter-module-small ## Run the tests
	cargo test \
	  --manifest-path=./hatchery/Cargo.toml \
	  --color=always

.PHONY: test modules assert-counter-module-small

COUNTER_MODULE_BYTE_SIZE_LIMIT = 512

assert-counter-module-small: modules
	@test `wc -c modules/target/stripped/counter.wasm | cut -f1 -d' '` -lt $(COUNTER_MODULE_BYTE_SIZE_LIMIT);
