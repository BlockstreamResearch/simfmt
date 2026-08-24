# Format code
fmt:
    cargo fmt --all

# Check if code is formatted
fmtcheck:
    cargo fmt --all -- --check

# Run code linter
lint:
    cargo clippy --all-targets --all-features --workspace -- --deny warnings

# Build code with all feature combinations
build_features:
    cargo hack check --feature-powerset --no-dev-deps

# Build code with all feature combinations
licenses:
    cargo deny check bans licenses sources

# Run unit tests
test:
    cargo test --workspace --all-features

# Check code (CI)
check:
    cargo --version
    rustc --version
    just fmtcheck
    just lint
    just build_features
    just test
    just licenses

# Remove all temporary files
clean:
    rm -rf target
