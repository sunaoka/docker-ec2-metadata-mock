IMAGE := sunaoka/ec2-metadata-mock
VERSION := $(shell cargo pkgid | sed 's/.*@//')

BUILDER := docker-ec2-metadata-mock-builder
PLATFORM := linux/arm64,linux/amd64

all:
	@awk -F':.*##' '/^[[:alnum:]_-]+:.*##/ { printf "\033[36m%-10s\033[0m%s\n", $$1, $$2 }' $(MAKEFILE_LIST)

test:  ## Run Rust tests
	cargo test --locked

coverage:  ## Run Rust tests with coverage
	cargo +nightly llvm-cov --all-features --workspace --branch --open

format:  ## Format the code
	cargo fmt --all -v

lint:  ## Lint the code
	cargo clippy --all-targets --all-features --locked -- -D warnings
	hadolint Dockerfile --ignore DL3018

build:  ## Build the Docker image
	(docker buildx ls | grep $(BUILDER)) || docker buildx create --name $(BUILDER)
	docker buildx use $(BUILDER)
	docker buildx build --rm --no-cache --platform $(PLATFORM) -t $(IMAGE):$(VERSION) -t $(IMAGE):latest --push .
	docker buildx rm $(BUILDER)

clean:  ## Clean up build artifacts
	cargo clean -vvv

bump:  ## Bump the version in Cargo.toml
	cargo set-version --bump patch

.PHONY: all test coverage format lint build clean bump
