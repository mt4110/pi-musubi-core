import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../../app/widgets/language_toggle_action.dart';
import '../../../app/widgets/musubi_pressable.dart';
import '../../../repositories/auth/auth_session_controller.dart';
import '../models/demo_match_profile.dart';

class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(authSessionControllerProvider).valueOrNull;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Serious Matches'),
        actions: [
          const LanguageToggleAction(),
          IconButton(
            tooltip: 'Sign out',
            onPressed: () {
              ref.read(authSessionControllerProvider.notifier).signOut();
            },
            icon: const Icon(Icons.logout_rounded),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(24, 20, 24, 32),
        children: [
          _HomeHero(sessionName: session?.displayName ?? '@pi_guest'),
          const SizedBox(height: 24),
          MusubiSurfaceCard(
            color: const Color(0xFF101821),
            child: Row(
              children: [
                const Icon(Icons.public_rounded, color: Color(0xFFE2B76A)),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    'Realmの立ち上げ申請を見る',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                MusubiGhostButton(
                  label: '開く',
                  onPressed: () => context.push('/realms/bootstrap'),
                ),
              ],
            ),
          ),
          const SizedBox(height: 24),
          Text(
            '今すぐ会話を始められる相手',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 8),
          Text(
            'Bot と冷やかしを減らすため、アプローチ時に 10 Pi のデポジットを入れる前提です。',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          const SizedBox(height: 18),
          for (final profile in demoMatchProfiles) ...[
            _DiscoveryCard(profile: profile),
            const SizedBox(height: 16),
          ],
        ],
      ),
    );
  }
}

class _HomeHero extends StatelessWidget {
  const _HomeHero({required this.sessionName});

  final String sessionName;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(28),
        gradient: const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [Color(0xFF181E26), Color(0xFF10151B), Color(0xFF33291B)],
        ),
        border: Border.all(color: const Color(0x1FFFFFFF)),
        boxShadow: musubiAmbientGlow(
          color: const Color(0xFFE2B76A),
          opacity: 0.1,
          blurRadius: 40,
          spreadRadius: 2,
          offset: const Offset(0, 20),
        ),
      ),
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Pi deposit matchmaking',
            style: Theme.of(
              context,
            ).textTheme.labelSmall?.copyWith(letterSpacing: 1.8),
          ),
          const SizedBox(height: 12),
          Text(
            '約束を守る人だけが残る、まっすぐな出会い。',
            style: Theme.of(context).textTheme.headlineSmall,
          ),
          const SizedBox(height: 12),
          Text(
            '$sessionName と同じ温度感で会える相手を、今日の候補だけに絞りました。',
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          const SizedBox(height: 20),
          Wrap(
            spacing: 10,
            runSpacing: 10,
            children: const [
              _HeroChip(label: '10 Pi deposit'),
              _HeroChip(label: 'No bot'),
              _HeroChip(label: 'No no-show'),
            ],
          ),
        ],
      ),
    );
  }
}

class _HeroChip extends StatelessWidget {
  const _HeroChip({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: const Color(0x12FFFFFF),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: const Color(0x18FFFFFF)),
      ),
      child: Text(
        label,
        style: Theme.of(
          context,
        ).textTheme.labelMedium?.copyWith(color: const Color(0xFFF3EBDD)),
      ),
    );
  }
}

class _DiscoveryCard extends StatelessWidget {
  const _DiscoveryCard({required this.profile});

  final DemoMatchProfile profile;

  @override
  Widget build(BuildContext context) {
    return MusubiPressable(
      onTap: () => context.push('/detail/${profile.id}'),
      child: MusubiSurfaceCard(
        padding: EdgeInsets.zero,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ClipRRect(
              borderRadius: const BorderRadius.vertical(
                top: Radius.circular(24),
              ),
              child: SizedBox(
                height: 220,
                width: double.infinity,
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    _ProfileImage(photoUrl: profile.photoUrl),
                    const DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          colors: [
                            Color(0x05000000),
                            Color(0x22000000),
                            Color(0xBB090909),
                          ],
                        ),
                      ),
                    ),
                    Positioned(
                      left: 18,
                      right: 18,
                      bottom: 18,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            '${profile.name}, ${profile.age}',
                            style: Theme.of(context)
                                .textTheme
                                .titleLarge
                                ?.copyWith(fontWeight: FontWeight.w700),
                          ),
                          const SizedBox(height: 6),
                          Text(
                            profile.headline,
                            style: Theme.of(context).textTheme.bodyMedium,
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 18, 18, 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '${profile.city}  •  ${profile.intent}',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  const SizedBox(height: 12),
                  Text(
                    profile.bio,
                    style: Theme.of(context).textTheme.bodyLarge,
                  ),
                  const SizedBox(height: 14),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      for (final hobby in profile.hobbies)
                        _HobbyChip(label: hobby),
                    ],
                  ),
                  const SizedBox(height: 18),
                  const Row(
                    children: [
                      Icon(
                        Icons.account_balance_wallet_outlined,
                        size: 18,
                        color: Color(0xFFE2B76A),
                      ),
                      SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          'デポジット前提で本気度を可視化',
                          style: TextStyle(
                            color: Color(0xFFF3EBDD),
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      Icon(Icons.chevron_right_rounded),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _HobbyChip extends StatelessWidget {
  const _HobbyChip({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
      decoration: BoxDecoration(
        color: const Color(0x12FFFFFF),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: const Color(0x14FFFFFF)),
      ),
      child: Text(label, style: Theme.of(context).textTheme.labelMedium),
    );
  }
}

class _ProfileImage extends StatelessWidget {
  const _ProfileImage({required this.photoUrl});

  final String photoUrl;

  @override
  Widget build(BuildContext context) {
    return Image.network(
      photoUrl,
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
              size: 54,
              color: Color(0xFFF3EBDD),
            ),
          ),
        );
      },
      loadingBuilder: (context, child, progress) {
        if (progress == null) {
          return child;
        }
        return const Center(child: CircularProgressIndicator());
      },
    );
  }
}
