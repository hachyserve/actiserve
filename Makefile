.PHONY: up
up:
	RUST_LOG=hyper=error,actiserve=debug cargo run -- --config-path resources/config.example.yaml

.PHONY: test-all
test-all:
	@echo "Make sure to run 'make up' first"
	BASE_URL='http://127.0.0.1:4242' cargo test --features need_local_server $(ARGS)

.PHONY: test-all-verbose
test-all-verbose:
	@echo "Make sure to run 'make up' first"
	BASE_URL='http://127.0.0.1:4242' cargo test --features need_local_server --verbose $(ARGS)
