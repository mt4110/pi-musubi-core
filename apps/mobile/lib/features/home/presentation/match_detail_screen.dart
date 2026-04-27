import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../../app/widgets/musubi_pressable.dart';
import '../../../core/errors/app_exception.dart';
import '../../../core/utils/random_hex.dart';
import '../../../features/promise/models/promise_models.dart';
import '../../../repositories/auth/auth_session_controller.dart';
import '../../../repositories/repository_providers.dart';
import '../models/demo_match_profile.dart';

class MatchDetailScreen extends ConsumerStatefulWidget {
  const MatchDetailScreen({super.key, required this.profileId});

  final String profileId;

  @override
  ConsumerState<MatchDetailScreen> createState() => _MatchDetailScreenState();
}

class _MatchDetailScreenState extends ConsumerState<MatchDetailScreen> {
  bool _isSubmitting = false;
  String? _pendingIdempotencyKey;

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
            MusubiPrimaryButton(
              label: _isSubmitting
                  ? 'Promise を作成しています...'
                  : 'Promise を作成して進む（10 Pi）',
              icon: Icons.handshake_rounded,
              isBusy: _isSubmitting,
              onPressed: _isSubmitting ? null : () => _createPromise(profile),
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
                              style: Theme.of(context)
                                  .textTheme
                                  .headlineSmall
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
                        color: const Color(0xFF111A16),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              'Promise preview',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                            const SizedBox(height: 12),
                            Text(
                              profile.promisePurpose,
                              style: Theme.of(context).textTheme.bodyLarge,
                            ),
                            const SizedBox(height: 12),
                            _PromisePreviewRow(
                              label: 'Time',
                              value: profile.promiseTimeWindow,
                            ),
                            const SizedBox(height: 8),
                            _PromisePreviewRow(
                              label: 'Place',
                              value: profile.promiseVenueSummary,
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 18),
                      MusubiSurfaceCard(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '約束にする前の確認',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                            const SizedBox(height: 12),
                            Text(
                              '${profile.city} で会う前提を、まずは bounded な Promise として記録します。',
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
                              'Promise rule',
                              style: Theme.of(context).textTheme.titleMedium,
                            ),
                            const SizedBox(height: 12),
                            Text(
                              '10 Pi のデポジットは、相手へのアクセス権ではありません。約束を雑に扱わないための預かりです。',
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

  Future<void> _createPromise(DemoMatchProfile profile) async {
    final session = ref.read(authSessionControllerProvider).valueOrNull;
    if (session == null) {
      _showSnack('サインイン状態を確認できませんでした。もう一度サインインしてください。');
      return;
    }

    setState(() => _isSubmitting = true);
    _pendingIdempotencyKey ??=
        'promise-ui-${session.userId}-${profile.id}-${randomHex(bytes: 8)}';
    try {
      final response =
          await ref.read(promiseRepositoryProvider).createPromiseIntent(
                CreatePromiseIntentRequest(
                  internalIdempotencyKey: _pendingIdempotencyKey!,
                  realmId: profile.realmId,
                  counterpartyAccountId: profile.counterpartyAccountId,
                  depositAmountMinorUnits: 10000,
                  currencyCode: 'PI',
                ),
              );
      if (!mounted) {
        return;
      }
      final uri = Uri(
        path: '/promises/${response.promiseIntentId}',
        queryParameters: {
          'settlementCaseId': response.settlementCaseId,
        },
      );
      context.push(
        uri.toString(),
        extra: {
          'created': true,
          'replayed': response.replayedIntent,
        },
      );
    } catch (error) {
      if (!mounted) {
        return;
      }
      final appError = AppExceptionMapper.fromObject(error);
      _showSnack(appError.message);
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }

  void _showSnack(String message) {
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(SnackBar(content: Text(message)));
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

class _PromisePreviewRow extends StatelessWidget {
  const _PromisePreviewRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 58,
          child: Text(label, style: Theme.of(context).textTheme.labelMedium),
        ),
        Expanded(
          child: Text(value, style: Theme.of(context).textTheme.bodyMedium),
        ),
      ],
    );
  }
}
