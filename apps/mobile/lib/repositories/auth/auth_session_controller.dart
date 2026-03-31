import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../core/errors/app_exception.dart';
import '../repository_providers.dart';
import 'pi_auth_session.dart';

final authSessionControllerProvider =
    AsyncNotifierProvider<AuthSessionController, PiAuthSession?>(
      AuthSessionController.new,
    );

class AuthSessionController extends AsyncNotifier<PiAuthSession?> {
  @override
  Future<PiAuthSession?> build() async {
    final repository = ref.read(authRepositoryProvider);
    try {
      final session = await repository.getCurrentSession();
      if (session != null) {
        return session;
      }
      return repository.trySilentSignIn();
    } catch (_) {
      return null;
    }
  }

  Future<AppException?> signInWithPi() async {
    final previous = state.valueOrNull;
    state = const AsyncLoading();
    final result = await AsyncValue.guard(
      () => ref.read(authRepositoryProvider).signInWithPi(),
    );
    if (result.hasError) {
      state = AsyncData(previous);
      final error = result.error;
      if (error is AppException) {
        return error;
      }
      return error == null
          ? const UnknownAppException()
          : AppExceptionMapper.fromObject(error);
    }
    state = result;
    return null;
  }

  Future<void> signOut() async {
    final previous = state.valueOrNull;
    state = const AsyncLoading();
    final result = await AsyncValue.guard(() async {
      await ref.read(authRepositoryProvider).signOut();
      return null;
    });
    if (result.hasError) {
      state = AsyncData(previous);
      return;
    }
    state = const AsyncData(null);
  }

  Future<void> refreshSession() async {
    final repository = ref.read(authRepositoryProvider);
    final result = await AsyncValue.guard(repository.getCurrentSession);
    if (result.hasError) {
      return;
    }
    state = result;
  }
}
