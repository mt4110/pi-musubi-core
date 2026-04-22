import '../../core/errors/app_exception.dart';
import '../../features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'realm_bootstrap_repository.dart';

class DummyRealmBootstrapRepository implements RealmBootstrapRepository {
  final _requests = <String, RealmRequestView>{};

  @override
  Future<RealmRequestView> createRealmRequest(
    CreateRealmRequestDraft draft,
  ) async {
    final requestId = 'realm-request-${_requests.length + 1}';
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

String? _trimmedOrNull(String? value) {
  final normalized = value?.trim();
  if (normalized == null || normalized.isEmpty) {
    return null;
  }
  return normalized;
}
