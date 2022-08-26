all: test-modules

COUNTER_MODULE_BYTE_SIZE_LIMIT = 1024

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: strip-modules test-modules assert-counter-module-small ## Run the modules' tests
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

RELEASE_MODULES = $(wildcard modules/target/wasm32-unknown-unknown/release/*.wasm)
STRIPPED_MODULES = $(subst modules/target/wasm32-unknown-unknown/release/,, $(RELEASE_MODULES))

$(STRIPPED_MODULES):
	mkdir -p modules/target/stripped;
	wasm-tools strip -a ./modules/target/wasm32-unknown-unknown/release/$@ -o ./modules/target/stripped/$@

strip-modules: $(STRIPPED_MODULES)

assert-counter-module-small:
	@test `wc -c ./modules/target/stripped/counter.wasm | cut -f1 -d' '` -lt $(COUNTER_MODULE_BYTE_SIZE_LIMIT);

.PHONY: all test-modules $(STRIPPED_MODULES)
