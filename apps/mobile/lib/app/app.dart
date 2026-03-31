import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/l10n/app_localizations.dart';

import '../l10n/locale_notifier.dart';
import 'router.dart';
import 'theme.dart';
import 'widgets/ambient_particles.dart';

class MusubiApp extends ConsumerWidget {
  const MusubiApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final locale = ref.watch(localeNotifierProvider);
    final router = ref.watch(goRouterProvider);

    return MaterialApp.router(
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      debugShowCheckedModeBanner: false,
      locale: locale,
      theme: musubiTheme,
      darkTheme: musubiTheme,
      themeMode: ThemeMode.dark,
      scrollBehavior: const _MusubiScrollBehavior(),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      routerConfig: router,
      builder: (context, child) {
        return Stack(
          fit: StackFit.expand,
          children: [
            const ColoredBox(color: Color(0xFF000000)),
            const AmbientParticles(),
            GestureDetector(
              behavior: HitTestBehavior.translucent,
              onTap: () {
                final focusScope = FocusScope.of(context);
                if (!focusScope.hasPrimaryFocus &&
                    focusScope.focusedChild != null) {
                  focusScope.unfocus();
                }
              },
              child: child ?? const SizedBox.shrink(),
            ),
          ],
        );
      },
    );
  }
}

class _MusubiScrollBehavior extends MaterialScrollBehavior {
  const _MusubiScrollBehavior();

  @override
  ScrollPhysics getScrollPhysics(BuildContext context) {
    return const BouncingScrollPhysics(parent: AlwaysScrollableScrollPhysics());
  }

  @override
  Widget buildOverscrollIndicator(
    BuildContext context,
    Widget child,
    ScrollableDetails details,
  ) {
    return child;
  }
}
