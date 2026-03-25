import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:shared_preferences/shared_preferences.dart';

const authJwtStorageKey = 'musubi_auth_jwt';

final authTokenStorageProvider = Provider<AuthTokenStorage>((ref) {
  return AuthTokenStorage();
});

class AuthTokenStorage {
  String? _memoryToken;

  Future<String?> readToken() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final token = prefs.getString(authJwtStorageKey);
      if (token != null && token.isNotEmpty) {
        _memoryToken = token;
        return token;
      }
    } catch (_) {
      // Fall back to in-memory token on web runtimes where persistence is unavailable.
    }
    return _memoryToken;
  }

  Future<void> writeToken(String token) async {
    _memoryToken = token;
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(authJwtStorageKey, token);
    } catch (_) {
      // Best effort only.
    }
  }

  Future<void> clearToken() async {
    _memoryToken = null;
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.remove(authJwtStorageKey);
    } catch (_) {
      // Best effort only.
    }
  }
}
