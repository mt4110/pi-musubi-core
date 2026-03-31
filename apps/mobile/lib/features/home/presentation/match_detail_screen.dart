import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../../app/widgets/musubi_pressable.dart';
import '../../../core/services/pi_sdk_service.dart';
import '../models/demo_match_profile.dart';

class MatchDetailScreen extends ConsumerStatefulWidget {
  const MatchDetailScreen({super.key, required this.profileId});

  final String profileId;

  @override
  ConsumerState<MatchDetailScreen> createState() => _MatchDetailScreenState();
}

class _MatchDetailScreenState extends ConsumerState<MatchDetailScreen> {
  bool _isSubmitting = false;
  PiSdkPaymentResult? _paymentResult;

  @override
  Widget build(BuildContext context) {
    final profile = findDemoMatchProfileById(widget.profileId);
    if (profile == null) {
      return Scaffold(
        appBar: AppBar(),
        body: const Center(child: Text('プロフィールが見つかりませんでした。')),
      );
    }

    return Scaffold(
      extendBodyBehindAppBar: true,
      appBar: AppBar(backgroundColor: Colors.transparent),
      bottomNavigationBar: SafeArea(
        minimum: const EdgeInsets.fromLTRB(20, 0, 20, 20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (_paymentResult != null)
              Container(
                width: double.infinity,
                margin: const EdgeInsets.only(bottom: 12),
                padding: const EdgeInsets.all(14),
                decoration: BoxDecoration(
                  color: const Color(0x141CB86D),
                  borderRadius: BorderRadius.circular(18),
                  border: Border.all(color: const Color(0x221CB86D)),
                ),
                child: Text(
                  'Stub payment accepted: ${_paymentResult!.paymentId}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
            MusubiPrimaryButton(
              label: _isSubmitting
                  ? 'Pi デポジット処理を準備中...'
                  : 'デポジットして本気のアプローチ（10 Pi）',
              icon: Icons.lock_open_rounded,
              isBusy: _isSubmitting,
              onPressed: _isSubmitting ? null : () => _submitDeposit(profile),
            ),
          ],
        ),
      ),
      body: CustomScrollView(
        slivers: [
          SliverToBoxAdapter(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SizedBox(
                  height: 420,
                  width: double.infinity,
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      Image.network(
                        profile.photoUrl,
                        fit: BoxFit.cover,
                        errorBuilder: (context, error, stackTrace) {
                          return const DecoratedBox(
                            decoration: BoxDecoration(
                              gradient: LinearGradient(
                                begin: Alignment.topLeft,
                                end: Alignment.bottomRight,
                                colors: [Color(0xFF26303D), Color(0xFF141A21)],
                              ),
                            ),
                            child: Center(
                              child: Icon(
                                Icons.person_outline_rounded,
                                size: 64,
                                color: Color(0xFFF3EBDD),
                              ),
                            ),
                          );
                        },
                      ),
                      const DecoratedBox(
                        decoration: BoxDecoration(
                          gradient: LinearGradient(
                            begin: Alignment.topCenter,
                            end: Alignment.bottomCenter,
                            colors: [
                              Color(0x05000000),
                              Color(0x22000000),
                              Color(0xEE090909),
                            ],
                          ),
                        ),
                      ),
                      Positioned(
                        left: 24,
                        right: 24,
                        bottom: 28,
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '${profile.name}, ${profile.age}',
                              style: Theme.of(context).textTheme.headlineSmall
                                  ?.copyWith(fontWeight: FontWeight.w700),
                            ),
                            const SizedBox(height: 8),
                            Text(
                              profile.headline,
                              style: Theme.of(context).textTheme.bodyLarge,
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(24, 24, 24, 120),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _InfoBlock(title: 'About', body: profile.bio),
                      const SizedBox(height: 18),
                      _InfoBlock(title: 'Intent', body: profile.intent),
                      const SizedBox(height: 18),
                      MusubiSurfaceCard(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '相性の良さそうな時間',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                            const SizedBox(height: 12),
                            Text(
                              '${profile.city} で、今週末の夕方以降に会える想定です。',
                              style: Theme.of(context).textTheme.bodyLarge,
                            ),
                            const SizedBox(height: 16),
                            Wrap(
                              spacing: 8,
                              runSpacing: 8,
                              children: [
                                for (final hobby in profile.hobbies)
                                  Container(
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: 10,
                                      vertical: 7,
                                    ),
                                    decoration: BoxDecoration(
                                      color: const Color(0x12FFFFFF),
                                      borderRadius: BorderRadius.circular(999),
                                      border: Border.all(
                                        color: const Color(0x14FFFFFF),
                                      ),
                                    ),
                                    child: Text(
                                      hobby,
                                      style: Theme.of(
                                        context,
                                      ).textTheme.labelMedium,
                                    ),
                                  ),
                              ],
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 18),
                      MusubiSurfaceCard(
                        color: const Color(0xFF17120A),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              'Deposit rule',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                            const SizedBox(height: 12),
                            Text(
                              '10 Pi のデポジットは、冷やかし・Bot・ドタキャンを減らすための本気度シグナルです。',
                              style: Theme.of(context).textTheme.bodyLarge,
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _submitDeposit(DemoMatchProfile profile) async {
    setState(() => _isSubmitting = true);
    try {
      final result = await ref
          .read(piSdkServiceProvider)
          .createPayment(
            PiSdkPaymentRequest(
              amountPi: 10,
              memo: 'Serious approach deposit for ${profile.name}',
              recipientPiUid: profile.piUid,
              metadata: <String, dynamic>{
                'target_profile_id': profile.id,
                'target_name': profile.name,
              },
            ),
          );
      if (!mounted) {
        return;
      }
      setState(() => _paymentResult = result);
      final messenger = ScaffoldMessenger.of(context);
      messenger.hideCurrentSnackBar();
      messenger.showSnackBar(
        SnackBar(
          content: Text('Pi デポジットのスタブを実行しました。payment_id=${result.paymentId}'),
        ),
      );
    } catch (error) {
      if (!mounted) {
        return;
      }
      final messenger = ScaffoldMessenger.of(context);
      messenger.hideCurrentSnackBar();
      messenger.showSnackBar(
        SnackBar(content: Text('決済スタブの実行に失敗しました: $error')),
      );
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }
}

class _InfoBlock extends StatelessWidget {
  const _InfoBlock({required this.title, required this.body});

  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Text(body, style: Theme.of(context).textTheme.bodyLarge),
      ],
    );
  }
}
