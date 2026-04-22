import '../../core/errors/app_exception.dart';
import '../../features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'realm_bootstrap_repository.dart';

class DummyRealmBootstrapRepository implements RealmBootstrapRepository {
  final _requests = <String, RealmRequestView>{};
  final _recordsByKey = <String, _DummyRealmRequestRecord>{};

  @override
  Future<RealmRequestView> createRealmRequest(
    CreateRealmRequestDraft draft,
  ) async {
    final requestIdempotencyKey = draft.requestIdempotencyKey.trim();
    final fingerprint = _fingerprint(draft);
    final existing = _recordsByKey[requestIdempotencyKey];
    if (existing != null) {
      if (existing.fingerprint != fingerprint) {
        throw const BusinessException(
          message: '同じ操作キーで別のRealm申請は作れません。'
              'もう一度画面を開き直してください。',
        );
      }
      return existing.view;
    }

    final requestId = 'realm-request-${_recordsByKey.length + 1}';
    final view = RealmRequestView(
      realmRequestId: requestId,
      displayName: draft.displayName.trim(),
      slugCandidate: draft.slugCandidate.trim(),
      purposeText: draft.purposeText.trim(),
      venueContextSummary: draft.venueContextText.trim(),
      expectedMemberShapeSummary: draft.expectedMemberShapeText.trim(),
      bootstrapRationaleText: draft.bootstrapRationaleText.trim(),
      requestState: 'requested',
      reviewReasonCode: 'request_received',
      createdRealmId: null,
      proposedSponsorAccountId: _trimmedOrNull(draft.proposedSponsorAccountId),
      proposedStewardAccountId: _trimmedOrNull(draft.proposedStewardAccountId),
    );
    final record = _DummyRealmRequestRecord(
      view: view,
      fingerprint: fingerprint,
    );
    _recordsByKey[requestIdempotencyKey] = record;
    _requests[requestId] = view;
    return view;
  }

  @override
  Future<RealmRequestView> fetchRealmRequest(String realmRequestId) async {
    final view = _requests[realmRequestId];
    if (view == null) {
      throw const BusinessException(message: 'Realm申請を確認できませんでした。');
    }
    return view;
  }

  @override
  Future<RealmBootstrapSummaryBundle> fetchBootstrapSummary(
    String realmId,
  ) async {
    return RealmBootstrapSummaryBundle(
      realmRequest: null,
      bootstrapView: RealmBootstrapView(
        realmId: realmId,
        slug: 'tokyo-calm-bootstrap',
        displayName: 'Tokyo calm bootstrap',
        realmStatus: 'limited_bootstrap',
        admissionPosture: 'limited',
        corridorStatus: 'active',
        publicReasonCode: 'limited_bootstrap_active',
        sponsorDisplayState: 'sponsor_and_steward',
      ),
      admissionView: RealmAdmissionView(
        realmId: realmId,
        accountId: 'demo-account',
        admissionStatus: 'pending',
        admissionKind: 'review_required',
        publicReasonCode: 'review_required',
      ),
    );
  }
}

String _fingerprint(CreateRealmRequestDraft draft) {
  return [
    draft.displayName.trim(),
    draft.slugCandidate.trim(),
    draft.purposeText.trim(),
    draft.venueContextText.trim(),
    draft.expectedMemberShapeText.trim(),
    draft.bootstrapRationaleText.trim(),
    _trimmedOrNull(draft.proposedSponsorAccountId) ?? '',
    _trimmedOrNull(draft.proposedStewardAccountId) ?? '',
  ].join('|');
}

String? _trimmedOrNull(String? value) {
  final normalized = value?.trim();
  if (normalized == null || normalized.isEmpty) {
    return null;
  }
  return normalized;
}

class _DummyRealmRequestRecord {
  const _DummyRealmRequestRecord({
    required this.view,
    required this.fingerprint,
  });

  final RealmRequestView view;
  final String fingerprint;
}
