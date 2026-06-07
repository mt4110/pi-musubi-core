BACKEND_DIR ?= apps/backend

.PHONY: verify-local verify-local-http http-day1-smoke http-smoke http-happy-route-smoke http-funded-happy-route-smoke http-funded-replay-smoke http-funded-duplicate-receipt-smoke

verify-local:
	@$(MAKE) -C $(BACKEND_DIR) verify-local

verify-local-http:
	@$(MAKE) -C $(BACKEND_DIR) verify-local-http

http-day1-smoke:
	@$(MAKE) -C $(BACKEND_DIR) http-day1-smoke

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
