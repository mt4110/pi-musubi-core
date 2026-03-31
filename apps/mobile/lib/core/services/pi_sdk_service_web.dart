// ignore_for_file: avoid_print
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:web/web.dart' as web;

import 'pi_sdk_service_contract.dart';

const _piVersion = String.fromEnvironment(
  'PI_SDK_VERSION',
  defaultValue: '2.0',
);
const _forcedEnvironment = String.fromEnvironment(
  'MUSUBI_PI_ENV',
  defaultValue: 'auto',
);
const _authScopes = <String>['username', 'payments', 'wallet_address'];

PiSdkService createPiSdkService() => _WebPiSdkService();

class _WebPiSdkService implements PiSdkService {
  bool _initialized = false;

  @override
  bool get isAvailable => web.window.has('Pi');

  @override
  Future<PiSdkAuthPayload> authenticate({bool preferSilent = false}) async {
    final pi = _readPi();
    await _ensureInitialized(pi);

    if (preferSilent) {
      throw const PiSdkUnavailableException(
        'Silent Pi auth is not supported in the current web runtime.',
      );
    }

    web.console.log('Pi SDK: Starting authentication flow...'.toJS);
    final onIncompletePaymentFound = ((JSAny? payment) {
      web.console.warn(
        'Pi SDK WARNING: Incomplete payment found! $payment'.toJS,
      );
      // If we don't complete it via backend, the auth flow might be permanently blocked.
      // But at least we log it loudly so we know this is the cause.
      // TODO: Handle incomplete payment completion via backend.
    }).toJS;
    final scopeArgs = _authScopes
        .map((scope) => scope.toJS)
        .toList(growable: false)
        .toJS;

    JSAny? response;
    try {
      web.console.log('Pi SDK: Calling window.Pi.authenticate()'.toJS);
      response = await _resolvePromise(
        pi.callMethodVarArgs<JSAny?>('authenticate'.toJS, <JSAny?>[
          scopeArgs,
          onIncompletePaymentFound,
        ]),
      );
      web.console.log('Pi SDK: Promise resolved successfully'.toJS);
    } catch (e) {
      web.console.error('Pi SDK ERROR in authenticate promise: $e'.toJS);
      rethrow;
    }

    final authObject = _asJsObject(response);
    final userObject = _asJsObject(authObject?['user']);
    final uid = _firstNonEmpty([
      _readJsString(userObject, 'uid'),
      _readJsString(authObject, 'uid'),
    ]);
    final username = _firstNonEmpty([
      _readJsString(userObject, 'username'),
      _readJsString(authObject, 'username'),
    ]);
    final accessToken = _firstNonEmpty([
      _readJsString(authObject, 'accessToken'),
      _readJsString(authObject, 'access_token'),
      _readJsString(authObject, 'token'),
    ]);
    final walletAddress = _firstNonEmpty([
      _readJsString(userObject, 'wallet_address'),
      _readJsString(userObject, 'walletAddress'),
      _readJsString(authObject, 'wallet_address'),
      _readJsString(authObject, 'walletAddress'),
    ]);
    final kycStatus = _firstNonEmpty([
      _readJsString(userObject, 'kyc_status'),
      _readJsString(userObject, 'kycStatus'),
      _readJsString(authObject, 'kyc_status'),
      _readJsString(authObject, 'kycStatus'),
    ]);
    final kycVerified =
        _readJsBool(userObject, 'kyc_verified') ??
        _readJsBool(userObject, 'kycVerified') ??
        _readJsBool(authObject, 'kyc_verified') ??
        _readJsBool(authObject, 'kycVerified');

    if (uid == null || username == null || accessToken == null) {
      throw const PiSdkUnavailableException(
        'Pi.authenticate() response is invalid.',
      );
    }

    return PiSdkAuthPayload(
      uid: uid,
      username: username,
      accessToken: accessToken,
      walletAddress: walletAddress,
      profile: <String, dynamic>{
        'uid': uid,
        'username': username,
        if (walletAddress != null) 'wallet_address': walletAddress,
        if (kycStatus != null) 'kyc_status': kycStatus,
        if (kycVerified != null) 'kyc_verified': kycVerified,
      },
    );
  }

  @override
  Future<PiSdkPaymentResult> createPayment(PiSdkPaymentRequest request) async {
    final stamp = DateTime.now().millisecondsSinceEpoch;
    final availability = isAvailable ? 'available' : 'unavailable';
    web.console.log(
      'Pi SDK stub payment: ${request.amountPi} Pi -> ${request.recipientPiUid} ($availability)'
          .toJS,
    );
    await Future<void>.delayed(const Duration(milliseconds: 900));
    return PiSdkPaymentResult(
      paymentId: 'pi-web-stub-$stamp',
      amountPi: request.amountPi,
      memo: request.memo,
      recipientPiUid: request.recipientPiUid,
      status: 'stubbed',
      txId: 'pi-web-tx-$stamp',
      isStub: true,
    );
  }

  JSObject _readPi() {
    if (!isAvailable) {
      throw const PiSdkUnavailableException();
    }
    final pi = web.window['Pi'];
    if (!pi.isA<JSObject>()) {
      throw const PiSdkUnavailableException();
    }
    return pi;
  }

  Future<void> _ensureInitialized(JSObject pi) async {
    if (_initialized) {
      return;
    }
    final config = <String, dynamic>{
      'version': _piVersion,
      'sandbox': _resolveSandboxMode(),
    };
    final result = pi.callMethodVarArgs<JSAny?>('init'.toJS, <JSAny?>[
      config.jsify(),
    ]);
    await _resolvePromise(result);
    _initialized = true;
  }

  bool _resolveSandboxMode() {
    final search = web.window.location.search.toLowerCase();
    final referrer = web.document.referrer.toLowerCase();
    if (search.contains('sandbox=true')) {
      return true;
    }
    if (referrer.contains('sandbox.minepi.com') ||
        referrer.contains('sandbox=true')) {
      return true;
    }

    final forced = _forcedEnvironment.trim().toLowerCase();
    if (forced == 'sandbox') {
      return true;
    }
    if (forced == 'prod' || forced == 'production') {
      return false;
    }
    final host = web.window.location.host.toLowerCase();
    return host.contains('localhost') ||
        host.contains('127.0.0.1') ||
        host.contains('sandbox') ||
        referrer.contains('sandbox.minepi.com') ||
        referrer.contains('sandbox=true');
  }
}

Future<JSAny?> _resolvePromise(JSAny? value) async {
  if (value == null) {
    return null;
  }
  if (value.isA<JSObject>() && (value as JSObject).has('then')) {
    return (value as JSPromise<JSAny?>).toDart;
  }
  return value;
}

JSObject? _asJsObject(JSAny? value) {
  if (value == null || !value.isA<JSObject>()) {
    return null;
  }
  return value as JSObject;
}

String? _readJsString(JSObject? object, String key) {
  if (object == null || !object.has(key)) {
    return null;
  }
  final value = object[key];
  if (value == null) {
    return null;
  }
  try {
    final dartValue = value.dartify();
    final normalized = '$dartValue'.trim();
    if (normalized.isEmpty || normalized == 'null') {
      return null;
    }
    return normalized;
  } catch (_) {
    return null;
  }
}

bool? _readJsBool(JSObject? object, String key) {
  if (object == null || !object.has(key)) {
    return null;
  }
  final value = object[key];
  if (value == null) {
    return null;
  }
  try {
    final dartValue = value.dartify();
    if (dartValue is bool) {
      return dartValue;
    }
    final normalized = '$dartValue'.trim().toLowerCase();
    if (normalized == 'true' || normalized == '1' || normalized == 'verified') {
      return true;
    }
    if (normalized == 'false' || normalized == '0' || normalized == 'pending') {
      return false;
    }
  } catch (_) {
    return null;
  }
  return null;
}

String? _firstNonEmpty(List<Object?> candidates) {
  for (final candidate in candidates) {
    if (candidate == null) {
      continue;
    }
    final value = '$candidate'.trim();
    if (value.isNotEmpty && value != 'null') {
      return value;
    }
  }
  return null;
}
