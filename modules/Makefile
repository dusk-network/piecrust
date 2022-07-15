SUBDIRS := $(wildcard ./*/.)

all: $(SUBDIRS)

$(SUBDIRS):
	@cargo build \
		--release \
		--manifest-path=$@/Cargo.toml \
        -Z build-std=core,alloc,panic_abort \
        -Z build-std-features=panic_immediate_abort \
        --target wasm32-unknown-unknown

.PHONY: all $(SUBDIRS)
