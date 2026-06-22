# Parametric HRTF — build & run helpers.
#
#   make wasm-target   # one-time: add the wasm32 rustc target
#   make test          # run the DSP + adapter unit tests
#   make bundle        # build the wasm and assemble the .wclap bundle
#   make serve         # bundle (if needed) + serve the GUI test host
#   make dev           # bundle + serve in one go

PORT ?= 8000

.PHONY: wasm-target test test-rust test-js bundle serve dev clean

wasm-target:
	rustup target add wasm32-unknown-unknown

test: test-rust test-js

test-rust:
	cargo test -p phrtf_distance_proximity -p phrtf-webclap

test-js:
	node --test crates/phrtf-webclap/ui/protocol.test.mjs

bundle:
	cargo xtask bundle-webclap --release

serve:
	./scripts/serve.sh $(PORT)

dev: bundle serve

clean:
	cargo clean
	rm -rf dist web/parametric-hrtf.wclap.tar.gz
