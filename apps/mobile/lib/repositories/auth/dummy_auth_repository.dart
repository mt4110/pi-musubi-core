import 'auth_repository.dart';
import 'pi_auth_session.dart';

class DummyAuthRepository implements AuthRepository {
  PiAuthSession? _session;

  @override
  Future<PiAuthSession?> getCurrentSession() async {
    await Future.delayed(const Duration(milliseconds: 450));
    return _session;
  }

  @override
  Future<PiAuthSession> signInWithPi() async {
    await Future.delayed(const Duration(seconds: 2));
    _session = PiAuthSession(
      userId: 'pi-user-demo',
      piUid: 'pi-user-demo',
      displayName: '@pi_pioneer_2026',
    );
    return _session!;
  }

  @override
  Future<PiAuthSession?> trySilentSignIn() async {
    await Future.delayed(const Duration(milliseconds: 300));
    return _session;
  }

  @override
  Future<void> signOut() async {
    await Future.delayed(const Duration(seconds: 1));
    _session = null;
  }
}
