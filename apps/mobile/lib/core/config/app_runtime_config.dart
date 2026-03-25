import 'package:musubi_mobile/core/riverpod_compat.dart';

const _defaultUseDummyData = bool.fromEnvironment(
  'MUSUBI_USE_DUMMY_DATA',
  defaultValue: false,
);

final useDummyDataProvider = Provider<bool>((ref) {
  return _defaultUseDummyData;
});

final useApiRepositoriesProvider = Provider<bool>((ref) {
  return !ref.watch(useDummyDataProvider);
});
