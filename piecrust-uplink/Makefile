help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: ## Run tests
	@cargo test --no-default-features
	@cargo test --no-default-features --features serde

clippy: ## Run clippy
	@cargo clippy -- -D warnings
	@cargo clippy --no-default-features --features serde -- -D warnings

no-std: ## Build for wasm32 (no_std check)
	@cargo build --release --no-default-features --features serde --target wasm32-unknown-unknown

.PHONY: help test clippy no-std
