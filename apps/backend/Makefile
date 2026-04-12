COMPOSE ?= docker compose
ENV_FILE ?= .env

.PHONY: dev check db-bootstrap db-migrate db-status db-reset-local docker-up docker-down

dev:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi_backend

check:
	cargo check

db-bootstrap:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db bootstrap

db-migrate:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db migrate

db-status:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db status

db-reset-local:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db reset-local --confirm-reset-local

docker-up:
	$(COMPOSE) up --build

docker-down:
	$(COMPOSE) down
