# Justfile for common tasks
# Usage: just build | test | cov | lcov

# Build the project
build:
    cargo build

# Run the project
run:
    cargo run

# Run tests
test:
    cargo test

# Lint the code
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Format the code
fmt:
    cargo fmt --all -- --check

# Fix formatting
fix-fmt:
    cargo fmt --all

# Generate HTML coverage using cargo-llvm-cov
# rustup component add llvm-tools-preview
# cargo install cargo-llvm-cov
cov:
    cargo llvm-cov --workspace --html --open

# Produce lcov file for CI or uploading
# rustup component add llvm-tools-preview
# cargo install cargo-llvm-cov
lcov:
    cargo llvm-cov --workspace --lcov --output-path coverage/lcov.info
