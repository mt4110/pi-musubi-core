import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:musubi_mobile/core/riverpod_compat.dart';

import '../../../app/widgets/musubi_pressable.dart';
import '../../../core/errors/app_exception.dart';
import '../../../core/utils/random_hex.dart';
import '../../../repositories/auth/auth_session_controller.dart';
import '../../../repositories/repository_providers.dart';
import '../models/realm_bootstrap_models.dart';

class RealmBootstrapScreen extends ConsumerStatefulWidget {
  const RealmBootstrapScreen({super.key});

  @override
  ConsumerState<RealmBootstrapScreen> createState() =>
      _RealmBootstrapScreenState();
}

class _RealmBootstrapScreenState extends ConsumerState<RealmBootstrapScreen> {
  final _displayNameController = TextEditingController();
  final _slugController = TextEditingController();
  final _purposeController = TextEditingController();
  final _venueController = TextEditingController();
  final _memberShapeController = TextEditingController();
  final _rationaleController = TextEditingController();
  final _sponsorController = TextEditingController();
  final _stewardController = TextEditingController();
  final _realmIdController = TextEditingController();

  bool _isSubmitting = false;
  bool _isLoadingSummary = false;
  String? _pendingRequestKey;
  String? _pendingRequestFingerprint;
  RealmRequestView? _request;
  RealmBootstrapSummaryBundle? _summary;

  @override
  void dispose() {
    _displayNameController.dispose();
    _slugController.dispose();
    _purposeController.dispose();
    _venueController.dispose();
    _memberShapeController.dispose();
    _rationaleController.dispose();
    _sponsorController.dispose();
    _stewardController.dispose();
    _realmIdController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final request = _request;
    final summary = _summary;
    return Scaffold(
      appBar: AppBar(title: const Text('Realm bootstrap')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(24, 20, 24, 40),
        children: [
          Text(
            'Realmを静かに立ち上げる',
            style: Theme.of(context).textTheme.headlineSmall,
          ),
          const SizedBox(height: 10),
          Text(
            '申請、確認、限定受付の状態だけを扱います。',
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          const SizedBox(height: 22),
          MusubiSurfaceCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Realm request',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 16),
                _RealmTextField(
                  controller: _displayNameController,
                  label: 'Realm name',
                  icon: Icons.public_rounded,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _slugController,
                  label: 'slug',
                  icon: Icons.link_rounded,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _purposeController,
                  label: 'purpose',
                  icon: Icons.flag_outlined,
                  maxLines: 3,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _venueController,
                  label: 'venue / locality / context',
                  icon: Icons.place_outlined,
                  maxLines: 3,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _memberShapeController,
                  label: 'intended member pattern',
                  icon: Icons.groups_2_outlined,
                  maxLines: 3,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _rationaleController,
                  label: 'bootstrap rationale',
                  icon: Icons.auto_awesome_motion_outlined,
                  maxLines: 3,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _sponsorController,
                  label: 'sponsor account id',
                  icon: Icons.volunteer_activism_outlined,
                  requiredField: false,
                ),
                const SizedBox(height: 12),
                _RealmTextField(
                  controller: _stewardController,
                  label: 'Steward account id',
                  icon: Icons.verified_user_outlined,
                  requiredField: false,
                ),
                const SizedBox(height: 18),
                MusubiPrimaryButton(
                  label: _isSubmitting ? '送信しています...' : 'Realm申請を送る',
                  icon: Icons.send_rounded,
                  isBusy: _isSubmitting,
                  onPressed: _isSubmitting ? null : _submitRequest,
                ),
              ],
            ),
          ),
          if (request != null) ...[
            const SizedBox(height: 18),
            _RealmRequestPanel(request: request),
          ],
          const SizedBox(height: 18),
          MusubiSurfaceCard(
            color: const Color(0xFF101821),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Bootstrap summary',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 14),
                _RealmTextField(
                  controller: _realmIdController,
                  label: 'realm_id',
                  icon: Icons.tag_rounded,
                ),
                const SizedBox(height: 14),
                MusubiGhostButton(
                  label: _isLoadingSummary ? '確認しています...' : '状態を確認',
                  onPressed: _isLoadingSummary ? null : _loadSummary,
                ),
              ],
            ),
          ),
          if (summary != null) ...[
            const SizedBox(height: 18),
            _BootstrapSummaryPanel(summary: summary),
            const SizedBox(height: 18),
            _AdmissionRequestPanel(summary: summary),
            const SizedBox(height: 18),
            _OperatorSummaryPanel(view: summary.bootstrapView),
          ],
        ],
      ),
    );
  }

  Future<void> _submitRequest() async {
    final session = ref.read(authSessionControllerProvider).valueOrNull;
    if (session == null) {
      _showSnack('サインイン状態を確認できませんでした。');
      return;
    }
    final missing = _missingRequiredField();
    if (missing != null) {
      _showSnack('$missing を入力してください。');
      return;
    }

    final requestFingerprint = _currentRequestFingerprint();
    setState(() {
      _isSubmitting = true;
      if (_pendingRequestKey == null ||
          _pendingRequestFingerprint != requestFingerprint) {
        _pendingRequestKey = 'realm-ui-${randomHex(bytes: 16)}';
        _pendingRequestFingerprint = requestFingerprint;
      }
    });
    try {
      final request =
          await ref.read(realmBootstrapRepositoryProvider).createRealmRequest(
                CreateRealmRequestDraft(
                  displayName: _displayNameController.text,
                  slugCandidate: _slugController.text,
                  purposeText: _purposeController.text,
                  venueContextText: _venueController.text,
                  expectedMemberShapeText: _memberShapeController.text,
                  bootstrapRationaleText: _rationaleController.text,
                  proposedSponsorAccountId:
                      _trimmedOrNull(_sponsorController.text),
                  proposedStewardAccountId:
                      _trimmedOrNull(_stewardController.text),
                  requestIdempotencyKey: _pendingRequestKey!,
                ),
              );
      if (!mounted) {
        return;
      }
      setState(() {
        _request = request;
        _pendingRequestKey = null;
        _pendingRequestFingerprint = null;
        if (request.createdRealmId != null) {
          _realmIdController.text = request.createdRealmId!;
        }
      });
    } catch (error) {
      if (!mounted) {
        return;
      }
      _showSnack(AppExceptionMapper.fromObject(error).message);
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }

  String _currentRequestFingerprint() {
    return jsonEncode({
      'display_name': _displayNameController.text.trim(),
      'slug_candidate': _slugController.text.trim(),
      'purpose_text': _purposeController.text.trim(),
      'venue_context_text': _venueController.text.trim(),
      'expected_member_shape_text': _memberShapeController.text.trim(),
      'bootstrap_rationale_text': _rationaleController.text.trim(),
      'proposed_sponsor_account_id': _sponsorController.text.trim(),
      'proposed_steward_account_id': _stewardController.text.trim(),
    });
  }

  Future<void> _loadSummary() async {
    final realmId = _realmIdController.text.trim();
    if (realmId.isEmpty) {
      _showSnack('realm_id を入力してください。');
      return;
    }
    setState(() => _isLoadingSummary = true);
    try {
      final summary = await ref
          .read(realmBootstrapRepositoryProvider)
          .fetchBootstrapSummary(
            realmId,
          );
      if (!mounted) {
        return;
      }
      setState(() => _summary = summary);
    } catch (error) {
      if (!mounted) {
        return;
      }
      _showSnack(AppExceptionMapper.fromObject(error).message);
    } finally {
      if (mounted) {
        setState(() => _isLoadingSummary = false);
      }
    }
  }

  String? _missingRequiredField() {
    final fields = <String, TextEditingController>{
      'Realm name': _displayNameController,
      'slug': _slugController,
      'purpose': _purposeController,
      'venue / locality / context': _venueController,
      'intended member pattern': _memberShapeController,
      'bootstrap rationale': _rationaleController,
    };
    for (final entry in fields.entries) {
      if (entry.value.text.trim().isEmpty) {
        return entry.key;
      }
    }
    return null;
  }

  void _showSnack(String message) {
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(SnackBar(content: Text(message)));
  }
}

class _RealmTextField extends StatelessWidget {
  const _RealmTextField({
    required this.controller,
    required this.label,
    required this.icon,
    this.maxLines = 1,
    this.requiredField = true,
  });

  final TextEditingController controller;
  final String label;
  final IconData icon;
  final int maxLines;
  final bool requiredField;

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      maxLines: maxLines,
      textInputAction:
          maxLines == 1 ? TextInputAction.next : TextInputAction.newline,
      decoration: InputDecoration(
        labelText: requiredField ? label : '$label · optional',
        prefixIcon: Icon(icon),
      ),
    );
  }
}

String? _trimmedOrNull(String value) {
  final normalized = value.trim();
  return normalized.isEmpty ? null : normalized;
}

class _RealmRequestPanel extends StatelessWidget {
  const _RealmRequestPanel({required this.request});

  final RealmRequestView request;

  @override
  Widget build(BuildContext context) {
    return MusubiSurfaceCard(
      color: const Color(0xFF111A16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('申請を受け付けました', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 12),
          _RealmStatusRow(
            label: '状態',
            value: realmRequestStateLabel(request.requestState),
          ),
          _RealmStatusRow(label: 'Realm', value: request.displayName),
          _RealmStatusRow(label: 'slug', value: request.slugCandidate),
          _RealmStatusRow(label: '確認理由', value: request.reviewReasonCode),
          if (request.createdRealmId != null)
            _RealmStatusRow(label: 'realm_id', value: request.createdRealmId!),
          const SizedBox(height: 12),
          Text(
            '承認や参加状態はwriter-ownedな記録が反映された後に確定します。',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _BootstrapSummaryPanel extends StatelessWidget {
  const _BootstrapSummaryPanel({required this.summary});

  final RealmBootstrapSummaryBundle summary;

  @override
  Widget build(BuildContext context) {
    final view = summary.bootstrapView;
    final admission = summary.admissionView;
    return MusubiSurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(view.displayName, style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: 12),
          _RealmStatusRow(
            label: 'Realm状態',
            value: realmStatusLabel(view.realmStatus),
          ),
          _RealmStatusRow(
            label: '受付',
            value: admissionPostureLabel(view.admissionPosture),
          ),
          _RealmStatusRow(
            label: 'corridor',
            value: corridorStatusLabel(view.corridorStatus),
          ),
          _RealmStatusRow(
            label: '参加',
            value: admissionStatusLabel(admission?.admissionStatus ?? ''),
          ),
          const SizedBox(height: 12),
          Text(
            participantBootstrapCopy(summary),
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _AdmissionRequestPanel extends StatelessWidget {
  const _AdmissionRequestPanel({required this.summary});

  final RealmBootstrapSummaryBundle summary;

  @override
  Widget build(BuildContext context) {
    final admission = summary.admissionView;
    return MusubiSurfaceCard(
      color: const Color(0xFF101821),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Admission request',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 12),
          _RealmStatusRow(label: '申請者', value: '現在のアカウント'),
          _RealmStatusRow(
            label: 'スポンサー',
            value: sponsorDisplayStateLabel(
              summary.bootstrapView.sponsorDisplayState,
            ),
          ),
          _RealmStatusRow(
            label: '参加状態',
            value: admissionStatusLabel(admission?.admissionStatus ?? ''),
          ),
          _RealmStatusRow(
            label: '判定',
            value: admissionKindLabel(admission?.admissionKind ?? ''),
          ),
          _RealmStatusRow(
            label: 'queue',
            value: admissionQueueLabel(admission?.admissionStatus ?? ''),
          ),
        ],
      ),
    );
  }
}

class _OperatorSummaryPanel extends StatelessWidget {
  const _OperatorSummaryPanel({required this.view});

  final RealmBootstrapView view;

  @override
  Widget build(BuildContext context) {
    return MusubiSurfaceCard(
      color: const Color(0xFF17120A),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Operator / Steward review',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 12),
          Text(
            operatorBootstrapCopy(view),
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          const SizedBox(height: 12),
          Text(
            '内部メモ、内部ID、証跡の所在はこの面には出しません。',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

class _RealmStatusRow extends StatelessWidget {
  const _RealmStatusRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 92,
            child: Text(label, style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(
            child: Text(value, style: Theme.of(context).textTheme.bodyMedium),
          ),
        ],
      ),
    );
  }
}
