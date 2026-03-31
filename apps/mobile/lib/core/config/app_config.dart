import 'package:flutter/foundation.dart';

import 'web_platform_context_stub.dart'
    if (dart.library.html) 'web_platform_context_web.dart'
    as web_context;

const _definedApiBaseUrl = String.fromEnvironment(
  'API_BASE_URL',
  defaultValue: '',
);
const _defaultLocalApiBaseUrl = 'http://localhost:8088';
const _defaultProductionApiBaseUrl = String.fromEnvironment(
  'MUSUBI_PROD_API_BASE_URL',
  defaultValue: 'https://api.pi-musubi.com',
);

class AppConfig {
  const AppConfig._();

  static String get apiBaseUrl {
    final fromEnvironment = _definedApiBaseUrl.trim();
    if (fromEnvironment.isNotEmpty) {
      return fromEnvironment;
    }
    if (_isProductionWebHost()) {
      return _defaultProductionApiBaseUrl;
    }
    return _defaultLocalApiBaseUrl;
  }

  static const String sentryDsn = String.fromEnvironment(
    'SENTRY_DSN',
    defaultValue: '',
  );

  static const bool isStaticOnly = bool.fromEnvironment(
    'IS_STATIC_ONLY',
    defaultValue: false,
  );
}

bool _isProductionWebHost() {
  if (!kIsWeb) {
    return false;
  }
  final host = (web_context.currentHost() ?? '').trim().toLowerCase();
  if (host.isEmpty) {
    return false;
  }
  return !_isLocalHost(host);
}

bool _isLocalHost(String host) {
  return host == 'localhost' ||
      host.startsWith('localhost:') ||
      host == '127.0.0.1' ||
      host.startsWith('127.0.0.1:') ||
      host == '[::1]' ||
      host.startsWith('[::1]:');
}
