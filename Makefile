MODULES := $(subst modules/,, $(wildcard modules/*))

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

$(MODULES):
	mkdir -p ./target/stripped
	cargo build \
	  --release \
          --manifest-path=./modules/$@/Cargo.toml \
	  --color=always \
	  -Z build-std=core,alloc,panic_abort \
	  -Z build-std-features=panic_immediate_abort \
	  --target wasm32-unknown-unknown
	wasm-tools strip -a modules/$@/target/wasm32-unknown-unknown/release/$@.wasm -o target/stripped/$@.wasm

test: modules assert-counter-module-small ## Run the module tests
	cargo test \
	  --manifest-path=./hatchery/Cargo.toml \
	  --color=always

.PHONY: all $(MODULES)

COUNTER_MODULE_BYTE_SIZE_LIMIT = 512

assert-counter-module-small: $(MODULES)
	@test `wc -c ./target/stripped/counter.wasm | cut -f1 -d' '` -lt $(COUNTER_MODULE_BYTE_SIZE_LIMIT);
