import 'dart:ui';

import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _localePreferenceKey = 'musubi.locale_code';

final localeNotifierProvider = StateNotifierProvider<LocaleNotifier, Locale>((
  ref,
) {
  final notifier = LocaleNotifier();
  notifier.load();
  return notifier;
});

class LocaleNotifier extends StateNotifier<Locale> {
  LocaleNotifier() : super(const Locale('ja'));

  Future<void> load() async {
    final prefs = await SharedPreferences.getInstance();
    final stored = prefs.getString(_localePreferenceKey);
    final normalized = _normalizeLanguageCode(stored);
    if (normalized != null) {
      state = Locale(normalized);
    }
  }

  Future<void> setLanguageCode(String code) async {
    final normalized = _normalizeLanguageCode(code);
    if (normalized == null) {
      return;
    }
    if (state.languageCode == normalized) {
      return;
    }
    state = Locale(normalized);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_localePreferenceKey, normalized);
  }

  Future<void> toggle() async {
    await setLanguageCode(state.languageCode == 'ja' ? 'en' : 'ja');
  }

  static String? _normalizeLanguageCode(String? raw) {
    if (raw == null) {
      return null;
    }
    final normalized = raw.trim().toLowerCase();
    if (normalized == 'ja' || normalized == 'en') {
      return normalized;
    }
    return null;
  }
}
