import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import 'app/app.dart';

void main() {
  runApp(const ProviderScope(child: MusubiApp()));
}
