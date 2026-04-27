import 'package:flutter_test/flutter_test.dart';
import 'package:musubi_mobile/features/realm_bootstrap/models/realm_bootstrap_models.dart';

void main() {
  test('realm bootstrap parsing ignores internal fields not modeled by UI', () {
    final summary = RealmBootstrapSummaryBundle.fromJson({
      'realm_request': {
        'realm_request_id': 'request-1',
        'display_name': 'Tokyo slow coffee',
        'slug_candidate': 'tokyo-slow-coffee',
        'purpose_text': 'Small calm meetings.',
        'venue_context_json': {'summary': 'Tokyo cafe'},
        'expected_member_shape_json': {'summary': 'quiet and local'},
        'bootstrap_rationale_text': 'Start with a bounded group.',
        'request_state': 'approved',
        'review_reason_code': 'limited_bootstrap_active',
        'created_realm_id': 'realm-1',
        'reviewed_by_operator_id': 'operator-secret',
        'operator_note_internal': 'must not render',
      },
      'bootstrap_view': {
        'realm_id': 'realm-1',
        'slug': 'tokyo-slow-coffee',
        'display_name': 'Tokyo slow coffee',
        'realm_status': 'limited_bootstrap',
        'admission_posture': 'limited',
        'corridor_status': 'active',
        'public_reason_code': 'limited_bootstrap_active',
        'sponsor_display_state': 'sponsor_and_steward',
        'source_fact_count': 100,
        'raw_evidence_locator': 'private://evidence',
      },
      'admission_view': {
        'realm_id': 'realm-1',
        'account_id': 'account-1',
        'admission_status': 'pending',
        'admission_kind': 'review_required',
        'public_reason_code': 'review_required',
        'source_fact_id': 'source-secret',
      },
    });

    expect(summary.realmRequest?.realmRequestId, 'request-1');
    expect(summary.bootstrapView.displayName, 'Tokyo slow coffee');
    expect(summary.admissionView?.admissionStatus, 'pending');
    expect(participantBootstrapCopy(summary), isNot(contains('operator')));
    expect(participantBootstrapCopy(summary), isNot(contains('source')));
  });

  test('realm request parsing summarizes arbitrary context JSON', () {
    final request = RealmRequestView.fromJson({
      'realm_request_id': 'request-1',
      'display_name': 'Tokyo slow coffee',
      'slug_candidate': 'tokyo-slow-coffee',
      'purpose_text': 'Small calm meetings.',
      'venue_context_json': {
        'venue_type': 'cafe',
        'city': 'Tokyo',
      },
      'expected_member_shape_json': {
        'size': 'small',
        'locality': 'neighborhood',
      },
      'bootstrap_rationale_text': 'Start with a bounded group.',
      'request_state': 'approved',
      'review_reason_code': 'limited_bootstrap_active',
    });

    expect(request.venueContextSummary, 'city: Tokyo, venue_type: cafe');
    expect(
      request.expectedMemberShapeSummary,
      'locality: neighborhood, size: small',
    );
  });

  test('realm bootstrap copy stays calm and non-gamified', () {
    const summary = RealmBootstrapSummaryBundle(
      realmRequest: null,
      bootstrapView: RealmBootstrapView(
        realmId: 'realm-1',
        slug: 'realm-one',
        displayName: 'Realm one',
        realmStatus: 'limited_bootstrap',
        admissionPosture: 'review_required',
        corridorStatus: 'active',
        publicReasonCode: 'review_required',
        sponsorDisplayState: 'steward_present',
      ),
      admissionView: null,
    );

    final copy = participantBootstrapCopy(summary);
    expect(copy, contains('確認'));
    expect(copy, isNot(contains('DM')));
    expect(copy, isNot(contains('ランキング')));
    expect(copy, isNot(contains('boost')));
  });

  test('realm bootstrap labels keep participant copy bounded', () {
    expect(admissionKindLabel('corridor'), 'コリドー');
    expect(reviewReasonCodeLabel('duplicate_or_invalid'), '内容を確認中');
    expect(reviewReasonCodeLabel('operator_restriction'), '安全確認中');
    expect(reviewReasonCodeLabel('raw_internal_reason'), '確認中');
  });

  test('realm bootstrap parsing fails fast on missing required fields', () {
    expect(
      () => RealmBootstrapView.fromJson({
        'realm_id': 'realm-1',
        'slug': 'realm-one',
        'display_name': 'Realm one',
        'realm_status': 'limited_bootstrap',
        'admission_posture': 'review_required',
        'corridor_status': 'active',
        'public_reason_code': 'review_required',
      }),
      throwsFormatException,
    );
  });

  test('realm bootstrap summary parsing fails fast on malformed view shape', () {
    expect(
      () => RealmBootstrapSummaryBundle.fromJson({
        'realm_request': null,
        'bootstrap_view': null,
        'admission_view': null,
      }),
      throwsFormatException,
    );
  });

  test('realm bootstrap summary parsing fails fast on malformed admission view', () {
    expect(
      () => RealmBootstrapSummaryBundle.fromJson({
        'realm_request': null,
        'bootstrap_view': {
          'realm_id': 'realm-1',
          'slug': 'realm-one',
          'display_name': 'Realm one',
          'realm_status': 'limited_bootstrap',
          'admission_posture': 'review_required',
          'corridor_status': 'active',
          'public_reason_code': 'review_required',
          'sponsor_display_state': 'steward_present',
        },
        'admission_view': 'not-an-object',
      }),
      throwsFormatException,
    );
  });
}
