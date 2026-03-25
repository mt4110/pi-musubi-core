import 'pi_auth_session.dart';

abstract class AuthRepository {
  Future<PiAuthSession> signInWithPi();
  Future<PiAuthSession?> trySilentSignIn();
  Future<PiAuthSession?> getCurrentSession();
  Future<void> signOut();
}
