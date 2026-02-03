default: build_release

clean:
	@echo "Cleaning using cargo..."
	@cargo clean

check:
	@echo "Checking..."
	@cargo check

clippy:
	@echo "Running Clippy..."
	@cargo clippy -- -W clippy::pedantic

clippy_fix:
	@echo "Running Clippy fixes..."
	@cargo clippy --fix -- -W clippy::pedantic

build_dev:
	@echo "Building debug..."
	@cargo build --target wasm32-wasip2

build_release:
	@echo "Building release..."
	@cargo build --release --target wasm32-wasip2
