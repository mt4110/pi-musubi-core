DOCKER_COMPOSE ?= $(shell if docker compose version >/dev/null 2>&1; then printf 'docker compose'; elif command -v docker-compose >/dev/null 2>&1; then printf 'docker-compose'; else printf 'docker compose'; fi)
ENV_FILE ?= .env

.PHONY: dev check test env-check infra-up verify-local db-bootstrap db-migrate db-status db-reset-local docker-up docker-down

dev: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi_backend

check:
	cargo check

test: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo test --workspace

env-check:
	@if [ ! -f $(ENV_FILE) ]; then cp .env.example $(ENV_FILE); fi
	@missing=$$(awk -F= 'FNR==NR { if ($$1 ~ /^[A-Z0-9_]+$$/) required[$$1]=1; next } { if ($$1 ~ /^[A-Z0-9_]+$$/) present[$$1]=1 } END { for (key in required) if (!(key in present)) print key }' .env.example $(ENV_FILE)); \
	if [ -n "$$missing" ]; then \
		echo "missing required keys in $(ENV_FILE):"; \
		echo "$$missing"; \
		echo "copy missing defaults from .env.example"; \
		exit 1; \
	fi

infra-up: env-check
	$(DOCKER_COMPOSE) --env-file $(ENV_FILE) up -d postgres redis
	@for attempt in $$(seq 1 60); do \
		status=$$(docker inspect -f '{{.State.Health.Status}}' musubi_postgres 2>/dev/null || true); \
		if [ "$$status" = "healthy" ]; then exit 0; fi; \
		echo "waiting for postgres health ($$attempt/60): $${status:-missing}"; \
		sleep 1; \
	done; \
	echo "postgres did not become healthy"; \
	exit 1
	@for attempt in $$(seq 1 60); do \
		status=$$(docker inspect -f '{{.State.Health.Status}}' musubi_redis 2>/dev/null || true); \
		if [ "$$status" = "healthy" ]; then exit 0; fi; \
		echo "waiting for redis health ($$attempt/60): $${status:-missing}"; \
		sleep 1; \
	done; \
	echo "redis did not become healthy"; \
	exit 1

verify-local: infra-up db-bootstrap db-migrate db-status test

db-bootstrap: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db bootstrap

db-migrate: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db migrate

db-status: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db status

db-reset-local: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db reset-local --confirm-reset-local

docker-up:
	$(DOCKER_COMPOSE) --env-file $(ENV_FILE) up --build

docker-down:
	$(DOCKER_COMPOSE) down
