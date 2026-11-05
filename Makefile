.PHONY: dev build run test test-rust test-frontend test-scripts lint install clean release

# Use Git Bash for the scripts. From PowerShell, plain "bash" is WSL's bash, where git
# has no Windows credentials (a push then asks for user/password and fails).
GIT_BASH := $(wildcard C:/Program?Files/Git/bin/bash.exe)
BASH := $(if $(GIT_BASH),"$(GIT_BASH)",bash)

# Start development mode (hot-reload)
dev:
	npm --prefix ui run tauri dev

# Build the installer (target/release/bundle/nsis) and target/release/nahshot.exe
build:
	npm --prefix ui run tauri build

# Build and open the app
run: build
	./target/release/nahshot.exe

# Run all tests
test: test-frontend test-rust test-scripts

# Rust tests (needs ui/dist, built by test-frontend)
test-rust:
	cargo test --workspace

# Frontend type check, build and tests
test-frontend:
	npm --prefix ui run build
	npm --prefix ui test

# Run the release script tests (throwaway repos, no network)
test-scripts:
	$(BASH) scripts/release_test.sh

# Tag and push the next version; CI builds the installer and publishes the release.
#   make release            next patch
#   make release V=minor    or V=major, or V=1.2.0
release:
	$(BASH) scripts/release.sh $(V)

# Format check and clippy
lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

# Install frontend dependencies
install:
	npm --prefix ui install

# Clean build artifacts
clean:
	cargo clean
	rm -rf ui/dist dist-release
