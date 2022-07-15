MODULE_DIRS := $(wildcard ./modules/*/.)

all: test-contracts

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: test-contracts ## Run the contracts' tests
	@cargo test --release --manifest-path=./hatchery/Cargo.toml

test-contracts: $(MODULE_DIRS) ## Build the test contracts

$(MODULE_DIRS):
	@cargo build \
		--release \
		--manifest-path=$@/Cargo.toml \
        -Z build-std=core,alloc,panic_abort \
        -Z build-std-features=panic_immediate_abort \
        --target wasm32-unknown-unknown

.PHONY: all test-contracts $(MODULE_DIRS)
