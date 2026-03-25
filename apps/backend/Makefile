COMPOSE ?= docker compose
ENV_FILE ?= .env

.PHONY: dev check docker-up docker-down

dev:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run

check:
	cargo check

docker-up:
	$(COMPOSE) up --build

docker-down:
	$(COMPOSE) down
