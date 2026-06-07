BACKEND_DIR ?= apps/backend

.PHONY: verify-local verify-local-http guardrail-sweep http-day1-smoke http-day1-smoke-existing-infra http-smoke http-happy-route-smoke http-funded-happy-route-smoke http-funded-replay-smoke http-funded-duplicate-receipt-smoke

verify-local:
	@$(MAKE) -C $(BACKEND_DIR) verify-local

verify-local-http:
	@$(MAKE) -C $(BACKEND_DIR) verify-local-http

guardrail-sweep:
	@$(MAKE) -C $(BACKEND_DIR) guardrail-sweep

http-day1-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-day1-smoke

http-day1-smoke-existing-infra:
	@$(MAKE) -C $(BACKEND_DIR) http-day1-smoke-existing-infra

http-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-smoke

http-happy-route-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-happy-route-smoke

http-funded-happy-route-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-funded-happy-route-smoke

http-funded-replay-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-funded-replay-smoke

http-funded-duplicate-receipt-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-funded-duplicate-receipt-smoke
