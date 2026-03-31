// ignore_for_file: avoid_print
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/core/services/pi_sdk_service.dart';
import 'package:musubi_mobile/core/utils/random_hex.dart';

import '../../core/errors/app_exception.dart';

const _reviewerMockMode = bool.fromEnvironment(
  'MUSUBI_REVIEWER_MOCK_MODE',
  defaultValue: false,
);
const _reviewerPiUid = String.fromEnvironment(
  'MUSUBI_REVIEWER_PI_UID',
  defaultValue: 'pi-reviewer-apple',
);
const _reviewerUsername = String.fromEnvironment(
  'MUSUBI_REVIEWER_USERNAME',
  defaultValue: '@reviewer_mock',
);

class PiAuthIdentity {
  const PiAuthIdentity({
    required this.piUid,
    required this.username,
    required this.accessToken,
    this.walletAddress,
    this.profile = const <String, dynamic>{},
  });

  final String piUid;
  final String username;
  final String accessToken;
  final String? walletAddress;
  final Map<String, dynamic> profile;
}

abstract class PiAuthBridge {
  Future<PiAuthIdentity> authenticate({bool preferSilent = false});
}

class WebPiAuthBridge implements PiAuthBridge {
  WebPiAuthBridge(this._piSdkService);

  final PiSdkService _piSdkService;

  @override
  Future<PiAuthIdentity> authenticate({bool preferSilent = false}) async {
    if (!_piSdkService.isAvailable) {
      return _fallbackIdentity();
    }

    try {
      final auth = await _piSdkService.authenticate(preferSilent: preferSilent);
      return PiAuthIdentity(
        piUid: auth.uid,
        username: auth.username,
        accessToken: auth.accessToken,
        walletAddress: auth.walletAddress,
        profile: auth.profile,
      );
    } on PiSdkUnavailableException {
      return _fallbackIdentity();
    } catch (error) {
      final normalized = '$error'.toLowerCase();
      if (normalized.contains('cancel')) {
        throw const AuthenticationCancelledException();
      }
      rethrow;
    }
  }

  PiAuthIdentity _fallbackIdentity() {
    final random = randomHex();
    final piUid = _reviewerMockMode ? _reviewerPiUid : 'pi-user-web';
    final username = _reviewerMockMode ? _reviewerUsername : '@pi_pioneer_2026';
    return PiAuthIdentity(
      piUid: piUid,
      username: username,
      accessToken:
          'pi-web-stub-${DateTime.now().millisecondsSinceEpoch}-$random',
      profile: <String, dynamic>{
        'uid': piUid,
        'username': username,
        'stub': true,
        'reviewer_mock': _reviewerMockMode,
      },
    );
  }
}

final piAuthBridgeProvider = Provider<PiAuthBridge>((ref) {
  return WebPiAuthBridge(ref.watch(piSdkServiceProvider));
});
