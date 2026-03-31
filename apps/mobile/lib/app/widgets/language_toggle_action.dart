import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import 'musubi_pressable.dart';
import '../../l10n/locale_notifier.dart';

class LanguageToggleAction extends ConsumerWidget {
  const LanguageToggleAction({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final locale = ref.watch(localeNotifierProvider);
    final selected = locale.languageCode == 'en' ? 'en' : 'ja';

    return Padding(
      padding: const EdgeInsets.only(right: 4),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _LanguageButton(
            label: 'JA',
            selected: selected == 'ja',
            onTap: () =>
                ref.read(localeNotifierProvider.notifier).setLanguageCode('ja'),
          ),
          const SizedBox(width: 2),
          Text(
            '|',
            style: Theme.of(
              context,
            ).textTheme.bodySmall?.copyWith(color: Colors.black45),
          ),
          const SizedBox(width: 2),
          _LanguageButton(
            label: 'EN',
            selected: selected == 'en',
            onTap: () =>
                ref.read(localeNotifierProvider.notifier).setLanguageCode('en'),
          ),
        ],
      ),
    );
  }
}

class _LanguageButton extends StatelessWidget {
  const _LanguageButton({
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final textStyle = Theme.of(context).textTheme.labelLarge?.copyWith(
      color: selected
          ? colorScheme.onSurface
          : colorScheme.onSurface.withValues(alpha: 0.5),
      fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
      decoration: selected ? TextDecoration.underline : TextDecoration.none,
      decorationThickness: selected ? 2 : null,
    );

    return MusubiPressable(
      onTap: onTap,
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(8),
        color: selected ? colorScheme.onSurface.withValues(alpha: 0.08) : null,
      ),
      child: Text(label, style: textStyle),
    );
  }
}
