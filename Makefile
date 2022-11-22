.PHONY: test-all
test-all:
	@echo "Make sure to run 'cargo run' first"
	@BASE_URL='http://127.0.0.1:4242' cargo test --features need_local_server $(ARGS)
