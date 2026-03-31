// ignore_for_file: avoid_print
import '../../api/api_client.dart';
import '../../core/errors/app_exception.dart';
import '../../integrations/pi/pi_auth_bridge.dart';
import 'auth_token_storage.dart';
import 'auth_repository.dart';
import 'pi_auth_session.dart';

class ApiAuthRepository implements AuthRepository {
  ApiAuthRepository(this._apiClient, this._tokenStorage, this._piAuthBridge);

  final ApiClient _apiClient;
  final AuthTokenStorage _tokenStorage;
  final PiAuthBridge _piAuthBridge;

  @override
  Future<PiAuthSession?> getCurrentSession() async {
    return null;
  }

  @override
  Future<PiAuthSession> signInWithPi() async {
    return _signInWithPi(preferSilent: false);
  }

  @override
  Future<PiAuthSession?> trySilentSignIn() async {
    return null;
  }

  @override
  Future<void> signOut() async {
    await _tokenStorage.clearToken();
  }

  Future<PiAuthSession> _signInWithPi({required bool preferSilent}) async {
    try {
      final identity = await _piAuthBridge.authenticate(
        preferSilent: preferSilent,
      );
      final response = await _apiClient.dio.post<Map<String, dynamic>>(
        '/api/auth/pi',
        data: {
          'pi_uid': identity.piUid,
          'username': identity.username,
          'wallet_address': identity.walletAddress,
          'access_token': identity.accessToken,
          'profile': identity.profile,
        },
      );
      final data = response.data ?? const <String, dynamic>{};
      final token = data['token'] as String?;
      final user = (data['user'] as Map?)?.cast<String, dynamic>();
      if (token == null || token.isEmpty || user == null) {
        throw const UnknownAppException(message: '認証レスポンスが不正です。');
      }

      await _tokenStorage.writeToken(token);
      return PiAuthSession(
        userId: '${user['id']}',
        piUid: '${user['pi_uid'] ?? identity.piUid}',
        displayName: '${user['username']}',
      );
    } catch (error, stack) {
      print('ApiAuthRepository _signInWithPi FATAL ERROR: $error');
      print('Stack trace: $stack');
      throw AppExceptionMapper.fromObject(error);
    }
  }
}
