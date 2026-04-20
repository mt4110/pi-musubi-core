import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../../app/widgets/musubi_pressable.dart';
import '../../../core/errors/app_exception.dart';
import '../../../repositories/repository_providers.dart';
import '../models/promise_models.dart';

class PromiseStatusScreen extends ConsumerStatefulWidget {
  const PromiseStatusScreen({
    super.key,
    required this.promiseIntentId,
    this.settlementCaseId,
    this.replayedIntent = false,
  });

  final String promiseIntentId;
  final String? settlementCaseId;
  final bool replayedIntent;

  @override
  ConsumerState<PromiseStatusScreen> createState() =>
      _PromiseStatusScreenState();
}

class _PromiseStatusScreenState extends ConsumerState<PromiseStatusScreen> {
  late Future<PromiseStatusBundle> _future;

  @override
  void initState() {
    super.initState();
    _future = _load();
  }

  Future<PromiseStatusBundle> _load() {
    return ref.read(promiseRepositoryProvider).fetchPromiseStatus(
          widget.promiseIntentId,
          settlementCaseId: widget.settlementCaseId,
        );
  }

  void _retry() {
    setState(() {
      _future = _load();
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Promise')),
      body: FutureBuilder<PromiseStatusBundle>(
        future: _future,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          }
          if (snapshot.hasError) {
            final error = AppExceptionMapper.fromObject(
              snapshot.error ?? const UnknownAppException(),
            );
            return _PromiseStatusFrame(
              title: '約束の状態を確認できませんでした',
              subtitle: error.message,
              children: [
                MusubiGhostButton(label: '再読み込み', onPressed: _retry),
              ],
            );
          }

          final bundle = snapshot.data!;
          return _PromiseStatusFrame(
            title: widget.replayedIntent ? '同じ約束を確認しました' : '約束を作成しました',
            subtitle: '約束の進み具合だけを、落ち着いて確認できます。',
            children: [
              _StatusRow(
                label: '約束',
                value: promiseStatusLabel(bundle.promiseStatus),
              ),
              _StatusRow(
                label: '預かり',
                value: settlementStatusLabel(bundle.settlementStatus),
              ),
              _StatusRow(
                label: '証明',
                value: proofStatusLabel(bundle.proofStatus),
              ),
              _GuidancePanel(copy: participantNextActionCopy(bundle)),
              _CompletionPanel(bundle: bundle),
              MusubiGhostButton(label: '更新', onPressed: _retry),
            ],
          );
        },
      ),
    );
  }
}

class _PromiseStatusFrame extends StatelessWidget {
  const _PromiseStatusFrame({
    required this.title,
    required this.subtitle,
    required this.children,
  });

  final String title;
  final String subtitle;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(24, 24, 24, 40),
      children: [
        Text(title, style: Theme.of(context).textTheme.headlineSmall),
        const SizedBox(height: 10),
        Text(subtitle, style: Theme.of(context).textTheme.bodyLarge),
        const SizedBox(height: 22),
        for (final child in children) ...[
          child,
          if (child != children.last) const SizedBox(height: 14),
        ],
      ],
    );
  }
}

class _StatusRow extends StatelessWidget {
  const _StatusRow({
    required this.label,
    required this.value,
  });

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 92,
          child: Text(label, style: Theme.of(context).textTheme.labelMedium),
        ),
        Expanded(
          child: Text(value, style: Theme.of(context).textTheme.titleMedium),
        ),
      ],
    );
  }
}

class _GuidancePanel extends StatelessWidget {
  const _GuidancePanel({required this.copy});

  final String copy;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: const Color(0x1474A086),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: const Color(0x2274A086)),
      ),
      child: Text(copy, style: Theme.of(context).textTheme.bodyMedium),
    );
  }
}

class _CompletionPanel extends StatelessWidget {
  const _CompletionPanel({required this.bundle});

  final PromiseStatusBundle bundle;

  @override
  Widget build(BuildContext context) {
    final proofUnavailable = bundle.proofStatus == 'unavailable';
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: const Color(0x12FFFFFF),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: const Color(0x14FFFFFF)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('完了について', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          Text(
            proofUnavailable
                ? 'この画面の操作だけで完了は確定しません。証明や確認の準備が整うまで、状態だけを確認できます。'
                : '完了は証明や確認結果に基づいて扱われます。この画面だけで預かり金や評価は変わりません。',
            style: Theme.of(context).textTheme.bodyMedium,
          ),
        ],
      ),
    );
  }
}
