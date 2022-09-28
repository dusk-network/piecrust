help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

modules: ## Build WASM modules
	@RUSTFLAGS="-C link-args=-zstack-size=65536" \
	cargo build \
	  --release \
      --manifest-path=modules/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc,panic_abort \
	  -Z build-std-features=panic_immediate_abort \
	  --target wasm32-unknown-unknown
	@mkdir -p target/stripped
	@find target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" \
	 | xargs -I % basename % \
	 | xargs -I % wasm-tools strip -a \
	 	          target/wasm32-unknown-unknown/release/% \
	 	          -o target/stripped/%

test: modules assert-counter-module-small ## Run the tests
	@cargo test \
	  --manifest-path=./vmx/Cargo.toml \
	  --color=always

.PHONY: test modules assert-counter-module-small

COUNTER_MODULE_BYTE_SIZE_LIMIT = 512

assert-counter-module-small: modules
	@test `wc -c target/stripped/counter.wasm | sed 's/^[^0-9]*\([0-9]*\).*/\1/'` -lt $(COUNTER_MODULE_BYTE_SIZE_LIMIT);
