import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';
import 'package:musubi_mobile/l10n/app_localizations.dart';

import '../../../app/widgets/language_toggle_action.dart';
import '../../../app/widgets/musubi_pressable.dart';
import '../../../core/errors/app_exception.dart';
import '../../../repositories/auth/auth_session_controller.dart';

class PiSignInScreen extends ConsumerStatefulWidget {
  const PiSignInScreen({super.key});

  @override
  ConsumerState<PiSignInScreen> createState() => _PiSignInScreenState();
}

class _PiSignInScreenState extends ConsumerState<PiSignInScreen> {
  @override
  Widget build(BuildContext context) {
    final authAsync = ref.watch(authSessionControllerProvider);
    final isSigningIn = authAsync.isLoading;
    final l10n = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Pi Sign In'),
        actions: const [LanguageToggleAction()],
      ),
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(24, 24, 24, 32),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 520),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Pi deposit で、本気の出会いだけを残す。',
                    textAlign: TextAlign.center,
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                  const SizedBox(height: 14),
                  Text(
                    'Bot とドタキャンを減らすため、アプローチ時に 10 Pi を預けるシンプルな導線に絞りました。',
                    textAlign: TextAlign.center,
                    style: Theme.of(context).textTheme.bodyLarge,
                  ),
                  const SizedBox(height: 28),
                  MusubiSurfaceCard(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Happy route',
                          style: Theme.of(context).textTheme.labelSmall,
                        ),
                        const SizedBox(height: 12),
                        Text(
                          '1. Pi でサインイン',
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        const SizedBox(height: 8),
                        Text(
                          '2. 会いたい相手を選ぶ',
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        const SizedBox(height: 8),
                        Text(
                          '3. 10 Pi をデポジットして本気のアプローチ',
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 28),
                  MusubiPrimaryButton(
                    label: isSigningIn
                        ? l10n.signingInWithPi
                        : l10n.signInWithPi,
                    icon: Icons.account_circle_outlined,
                    isBusy: isSigningIn,
                    onPressed: isSigningIn ? null : _signIn,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _signIn() async {
    FocusScope.of(context).unfocus();
    final l10n = AppLocalizations.of(context);
    final error = await ref
        .read(authSessionControllerProvider.notifier)
        .signInWithPi();
    if (!mounted || error == null) {
      return;
    }
    final message = switch (error) {
      AuthenticationCancelledException() => l10n.signInCancelled,
      _ => '${l10n.signInFailed} \n\n[DEBUG ERROR]: ${error.toString()}',
    };
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      SnackBar(content: Text(message), duration: const Duration(seconds: 15)),
    );
  }
}
