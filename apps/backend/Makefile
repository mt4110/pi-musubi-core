DOCKER_COMPOSE ?= $(shell if docker compose version >/dev/null 2>&1; then printf 'docker compose'; elif command -v docker-compose >/dev/null 2>&1; then printf 'docker-compose'; else printf 'docker compose'; fi)
ENV_FILE ?= .env
HTTP_SMOKE_LOAD_ENV_FILE ?= true
HTTP_SMOKE_HOST ?= 127.0.0.1
HTTP_SMOKE_PORT ?= 18088
HTTP_SMOKE_BASE_URL ?= http://$(HTTP_SMOKE_HOST):$(HTTP_SMOKE_PORT)
HTTP_SMOKE_STARTUP_TIMEOUT_SECONDS ?= 120
HTTP_HAPPY_ROUTE_SMOKE_HOST ?= 127.0.0.1
HTTP_HAPPY_ROUTE_SMOKE_PORT ?= 18089
HTTP_HAPPY_ROUTE_SMOKE_BASE_URL ?= http://$(HTTP_HAPPY_ROUTE_SMOKE_HOST):$(HTTP_HAPPY_ROUTE_SMOKE_PORT)
HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS ?= 120
HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST ?= 127.0.0.1
HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT ?= 18090
HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL ?= http://$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST):$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT)
HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS ?= 120
HTTP_FUNDED_REPLAY_SMOKE_HOST ?= 127.0.0.1
HTTP_FUNDED_REPLAY_SMOKE_PORT ?= 18091
HTTP_FUNDED_REPLAY_SMOKE_BASE_URL ?= http://$(HTTP_FUNDED_REPLAY_SMOKE_HOST):$(HTTP_FUNDED_REPLAY_SMOKE_PORT)
HTTP_FUNDED_REPLAY_SMOKE_STARTUP_TIMEOUT_SECONDS ?= 120
HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_HOST ?= 127.0.0.1
HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_PORT ?= 18092
HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_BASE_URL ?= http://$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_HOST):$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_PORT)
HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_STARTUP_TIMEOUT_SECONDS ?= 120

.PHONY: dev check test guardrail-sweep guardrail-sweep-self-test env-check infra-up verify-local verify-local-http http-day1-smoke http-day1-smoke-existing-infra http-smoke http-smoke-existing-infra http-smoke-run http-happy-route-smoke http-happy-route-smoke-existing-infra http-funded-happy-route-smoke http-funded-happy-route-smoke-existing-infra http-funded-replay-smoke http-funded-replay-smoke-existing-infra http-funded-duplicate-receipt-smoke http-funded-duplicate-receipt-smoke-existing-infra db-bootstrap db-bootstrap-existing-infra db-migrate db-migrate-existing-infra db-status db-status-existing-infra db-reset-local docker-up docker-down

dev: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi_backend

check:
	cargo check

test: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo test --workspace

guardrail-sweep:
	@bash ./scripts/guardrail_sweep.sh

guardrail-sweep-self-test:
	@bash ./scripts/guardrail_sweep.sh --self-test

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

verify-local-http: verify-local
	@$(MAKE) http-smoke
	@$(MAKE) http-happy-route-smoke
	@$(MAKE) http-funded-happy-route-smoke
	@$(MAKE) http-funded-replay-smoke
	@$(MAKE) http-funded-duplicate-receipt-smoke

http-day1-smoke:
	@$(MAKE) infra-up
	@$(MAKE) db-bootstrap
	@$(MAKE) db-migrate
	@$(MAKE) db-status
	@$(MAKE) http-smoke
	@$(MAKE) http-happy-route-smoke
	@$(MAKE) http-funded-happy-route-smoke
	@$(MAKE) http-funded-replay-smoke
	@$(MAKE) http-funded-duplicate-receipt-smoke

http-day1-smoke-existing-infra:
	@$(MAKE) db-bootstrap-existing-infra
	@$(MAKE) db-migrate-existing-infra
	@$(MAKE) db-status-existing-infra
	@$(MAKE) http-smoke-existing-infra
	@$(MAKE) http-happy-route-smoke-existing-infra
	@$(MAKE) http-funded-happy-route-smoke-existing-infra
	@$(MAKE) http-funded-replay-smoke-existing-infra
	@$(MAKE) http-funded-duplicate-receipt-smoke-existing-infra

http-smoke: env-check
	@$(MAKE) http-smoke-run HTTP_SMOKE_LOAD_ENV_FILE=true

http-smoke-existing-infra:
	@$(MAKE) http-smoke-run HTTP_SMOKE_LOAD_ENV_FILE=false

http-smoke-run:
	@command -v curl >/dev/null 2>&1 || { echo "curl is required for http-smoke"; exit 1; }
	@backend_log=$$(mktemp -t musubi-backend-http-smoke-log.XXXXXX); \
	body_file=$$(mktemp -t musubi-backend-http-smoke-body.XXXXXX); \
	backend_pid=""; \
	trap 'status=$$?; if [ -n "$$backend_pid" ] && kill -0 "$$backend_pid" 2>/dev/null; then kill "$$backend_pid" >/dev/null 2>&1 || true; wait "$$backend_pid" 2>/dev/null || true; fi; rm -f "$$backend_log" "$$body_file"; exit $$status' EXIT INT TERM; \
	dotenv_skip_flag=false; \
	if [ "$(HTTP_SMOKE_LOAD_ENV_FILE)" = "true" ]; then set -a; . ./$(ENV_FILE); set +a; elif [ "$(HTTP_SMOKE_LOAD_ENV_FILE)" = "false" ]; then dotenv_skip_flag=true; else echo "HTTP_SMOKE_LOAD_ENV_FILE must be true or false"; exit 1; fi; \
	cargo build -p musubi_backend || exit 1; \
	if curl -fsS --max-time 2 "$(HTTP_SMOKE_BASE_URL)/health" > "$$body_file" 2>/dev/null; then \
		echo "HTTP smoke port already has a responding /health endpoint: $(HTTP_SMOKE_BASE_URL)"; \
		echo "stop the existing process or set HTTP_SMOKE_PORT to a free port"; \
		exit 1; \
	fi; \
	echo "starting backend for HTTP smoke at $(HTTP_SMOKE_BASE_URL)"; \
	MUSUBI_SKIP_DOTENV=$$dotenv_skip_flag APP_HOST=$(HTTP_SMOKE_HOST) PORT=$(HTTP_SMOKE_PORT) ./target/debug/musubi_backend > "$$backend_log" 2>&1 & \
	backend_pid=$$!; \
	ready=0; \
	for attempt in $$(seq 1 $(HTTP_SMOKE_STARTUP_TIMEOUT_SECONDS)); do \
		if curl -fsS --max-time 2 "$(HTTP_SMOKE_BASE_URL)/health" > "$$body_file" 2>/dev/null; then ready=1; break; fi; \
		if ! kill -0 "$$backend_pid" 2>/dev/null; then \
			echo "backend exited before HTTP smoke became ready"; \
			cat "$$backend_log"; \
			exit 1; \
		fi; \
		sleep 1; \
	done; \
	if [ "$$ready" != "1" ]; then \
		echo "backend did not become ready within $(HTTP_SMOKE_STARTUP_TIMEOUT_SECONDS)s"; \
		cat "$$backend_log"; \
		exit 1; \
	fi; \
	if ! kill -0 "$$backend_pid" 2>/dev/null; then \
		echo "backend exited after readiness probe succeeded"; \
		cat "$$backend_log"; \
		exit 1; \
	fi; \
	grep -q '"status":"ok"' "$$body_file" || { echo "/health did not report ok"; cat "$$body_file"; exit 1; }; \
	curl -fsS --max-time 5 "$(HTTP_SMOKE_BASE_URL)/api/launch/posture" > "$$body_file" || { echo "/api/launch/posture request failed"; exit 1; }; \
	grep -q '"launch_mode":' "$$body_file" || { echo "/api/launch/posture did not include launch_mode"; cat "$$body_file"; exit 1; }; \
	grep -q '"message_code":' "$$body_file" || { echo "/api/launch/posture did not include message_code"; cat "$$body_file"; exit 1; }; \
	curl -fsS --max-time 5 "$(HTTP_SMOKE_BASE_URL)/api/internal/ops/health" > "$$body_file" || { echo "/api/internal/ops/health request failed"; exit 1; }; \
	grep -q '"status":"ok"' "$$body_file" || { echo "/api/internal/ops/health did not report ok"; cat "$$body_file"; exit 1; }; \
	grep -q '"database":' "$$body_file" || { echo "/api/internal/ops/health did not include database"; cat "$$body_file"; exit 1; }; \
	curl -fsS --max-time 5 "$(HTTP_SMOKE_BASE_URL)/api/internal/ops/readiness" > "$$body_file" || { echo "/api/internal/ops/readiness request failed"; exit 1; }; \
	grep -q '"status":"ready"' "$$body_file" || { echo "/api/internal/ops/readiness did not report ready"; cat "$$body_file"; exit 1; }; \
	grep -q '"pending_count":0' "$$body_file" || { echo "/api/internal/ops/readiness did not report zero pending migrations"; cat "$$body_file"; exit 1; }; \
	echo "HTTP smoke passed at $(HTTP_SMOKE_BASE_URL)"

http-happy-route-smoke: env-check
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=true \
	HTTP_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_HAPPY_ROUTE_SMOKE_HOST)" \
	HTTP_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_HAPPY_ROUTE_SMOKE_PORT)" \
	HTTP_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_HAPPY_ROUTE_SMOKE_BASE_URL)" \
	HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	./scripts/http_happy_route_smoke.sh

http-happy-route-smoke-existing-infra:
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=false \
	HTTP_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_HAPPY_ROUTE_SMOKE_HOST)" \
	HTTP_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_HAPPY_ROUTE_SMOKE_PORT)" \
	HTTP_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_HAPPY_ROUTE_SMOKE_BASE_URL)" \
	HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	./scripts/http_happy_route_smoke.sh

http-funded-happy-route-smoke: env-check
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=true \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	./scripts/http_funded_happy_route_smoke.sh

http-funded-happy-route-smoke-existing-infra:
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=false \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	./scripts/http_funded_happy_route_smoke.sh

http-funded-replay-smoke: env-check
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=true \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_REPLAY_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_REPLAY_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_REPLAY_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_REPLAY_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID="http-funded-replay-smoke-initiator" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID="http-funded-replay-smoke-counterparty" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_REALM_ID="realm-http-funded-replay-smoke" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_REPLAY=true \
	./scripts/http_funded_happy_route_smoke.sh

http-funded-replay-smoke-existing-infra:
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=false \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_REPLAY_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_REPLAY_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_REPLAY_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_REPLAY_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID="http-funded-replay-smoke-initiator" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID="http-funded-replay-smoke-counterparty" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_REALM_ID="realm-http-funded-replay-smoke" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_REPLAY=true \
	./scripts/http_funded_happy_route_smoke.sh

http-funded-duplicate-receipt-smoke: env-check
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=true \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID="http-funded-duplicate-receipt-smoke-initiator" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID="http-funded-duplicate-receipt-smoke-counterparty" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_REALM_ID="realm-http-funded-duplicate-receipt-smoke" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_DUPLICATE_RECEIPT=true \
	./scripts/http_funded_happy_route_smoke.sh

http-funded-duplicate-receipt-smoke-existing-infra:
	@ENV_FILE="$(ENV_FILE)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE=false \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_HOST)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_PORT)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_BASE_URL)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS="$(HTTP_FUNDED_DUPLICATE_RECEIPT_SMOKE_STARTUP_TIMEOUT_SECONDS)" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID="http-funded-duplicate-receipt-smoke-initiator" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID="http-funded-duplicate-receipt-smoke-counterparty" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_REALM_ID="realm-http-funded-duplicate-receipt-smoke" \
	HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_DUPLICATE_RECEIPT=true \
	./scripts/http_funded_happy_route_smoke.sh

db-bootstrap: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db bootstrap

db-bootstrap-existing-infra:
	MUSUBI_SKIP_DOTENV=true cargo run -p musubi-ops -- db bootstrap

db-migrate: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db migrate

db-migrate-existing-infra:
	MUSUBI_SKIP_DOTENV=true cargo run -p musubi-ops -- db migrate

db-status: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db status

db-status-existing-infra:
	MUSUBI_SKIP_DOTENV=true cargo run -p musubi-ops -- db status

db-reset-local: env-check
	@set -a; . ./$(ENV_FILE); set +a; cargo run -p musubi-ops -- db reset-local --confirm-reset-local

docker-up:
	$(DOCKER_COMPOSE) --env-file $(ENV_FILE) up --build

docker-down:
	$(DOCKER_COMPOSE) down
