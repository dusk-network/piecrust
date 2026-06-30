help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

test: ## Run tests
	@cargo test --all-features --color=always
	@rustup component add --toolchain nightly miri
	@cargo +nightly miri test --all-features --color=always test_no_dangling_commit_index_reference

clippy: ## Run clippy
	@cargo clippy --all-features -- -D warnings
	@cargo clippy --no-default-features -- -D warnings

.PHONY: help test clippy
