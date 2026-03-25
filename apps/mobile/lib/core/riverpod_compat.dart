import 'package:flutter_riverpod/flutter_riverpod.dart';

export 'package:flutter_riverpod/flutter_riverpod.dart';
export 'package:flutter_riverpod/legacy.dart';

extension AsyncValueCompatX<T> on AsyncValue<T> {
  T? get valueOrNull => switch (this) {
    AsyncData(:final value) => value,
    _ => null,
  };
}
