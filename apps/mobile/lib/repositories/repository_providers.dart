import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../api/api_client.dart';
import '../core/config/app_runtime_config.dart';
import '../integrations/pi/pi_auth_bridge.dart';
import 'auth/api_auth_repository.dart';
import 'auth/auth_repository.dart';
import 'auth/auth_token_storage.dart';
import 'auth/dummy_auth_repository.dart';
import 'promise/api_promise_repository.dart';
import 'promise/dummy_promise_repository.dart';
import 'promise/promise_repository.dart';

final authRepositoryProvider = Provider<AuthRepository>((ref) {
  if (ref.watch(useApiRepositoriesProvider)) {
    return ApiAuthRepository(
      ref.watch(apiClientProvider),
      ref.watch(authTokenStorageProvider),
      ref.watch(piAuthBridgeProvider),
    );
  }
  return DummyAuthRepository();
});

final promiseRepositoryProvider = Provider<PromiseRepository>((ref) {
  if (ref.watch(useApiRepositoriesProvider)) {
    return ApiPromiseRepository(ref.watch(apiClientProvider));
  }
  return DummyPromiseRepository();
});
