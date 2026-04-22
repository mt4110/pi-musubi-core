class CreateRealmRequestDraft {
  const CreateRealmRequestDraft({
    required this.displayName,
    required this.slugCandidate,
    required this.purposeText,
    required this.venueContextText,
    required this.expectedMemberShapeText,
    required this.bootstrapRationaleText,
    required this.requestIdempotencyKey,
    this.proposedSponsorAccountId,
    this.proposedStewardAccountId,
  });

  final String displayName;
  final String slugCandidate;
  final String purposeText;
  final String venueContextText;
  final String expectedMemberShapeText;
  final String bootstrapRationaleText;
  final String requestIdempotencyKey;
  final String? proposedSponsorAccountId;
  final String? proposedStewardAccountId;

  Map<String, Object?> toJson() {
    return {
      'display_name': displayName.trim(),
      'slug_candidate': slugCandidate.trim(),
      'purpose_text': purposeText.trim(),
      'venue_context_json': {'summary': venueContextText.trim()},
      'expected_member_shape_json': {
        'summary': expectedMemberShapeText.trim(),
      },
      'bootstrap_rationale_text': bootstrapRationaleText.trim(),
      'proposed_sponsor_account_id': _trimmedOrNull(proposedSponsorAccountId),
      'proposed_steward_account_id': _trimmedOrNull(proposedStewardAccountId),
      'request_idempotency_key': requestIdempotencyKey,
    };
  }
}

class RealmRequestView {
  const RealmRequestView({
    required this.realmRequestId,
    required this.displayName,
    required this.slugCandidate,
    required this.purposeText,
    required this.venueContextSummary,
    required this.expectedMemberShapeSummary,
    required this.bootstrapRationaleText,
    required this.requestState,
    required this.reviewReasonCode,
    required this.createdRealmId,
    required this.proposedSponsorAccountId,
    required this.proposedStewardAccountId,
  });

  final String realmRequestId;
  final String displayName;
  final String slugCandidate;
  final String purposeText;
  final String venueContextSummary;
  final String expectedMemberShapeSummary;
  final String bootstrapRationaleText;
  final String requestState;
  final String reviewReasonCode;
  final String? createdRealmId;
  final String? proposedSponsorAccountId;
  final String? proposedStewardAccountId;

  factory RealmRequestView.fromJson(Map<String, dynamic> json) {
    return RealmRequestView(
      realmRequestId: _stringField(json, 'realm_request_id'),
      displayName: _stringField(json, 'display_name'),
      slugCandidate: _stringField(json, 'slug_candidate'),
      purposeText: _stringField(json, 'purpose_text'),
      venueContextSummary: _summaryFromJson(json['venue_context_json']),
      expectedMemberShapeSummary: _summaryFromJson(
        json['expected_member_shape_json'],
      ),
      bootstrapRationaleText: _stringField(
        json,
        'bootstrap_rationale_text',
      ),
      requestState: _stringField(json, 'request_state'),
      reviewReasonCode: _stringField(json, 'review_reason_code'),
      createdRealmId: _nullableString(json, 'created_realm_id'),
      proposedSponsorAccountId: _nullableString(
        json,
        'proposed_sponsor_account_id',
      ),
      proposedStewardAccountId: _nullableString(
        json,
        'proposed_steward_account_id',
      ),
    );
  }
}

class RealmBootstrapView {
  const RealmBootstrapView({
    required this.realmId,
    required this.slug,
    required this.displayName,
    required this.realmStatus,
    required this.admissionPosture,
    required this.corridorStatus,
    required this.publicReasonCode,
    required this.sponsorDisplayState,
  });

  final String realmId;
  final String slug;
  final String displayName;
  final String realmStatus;
  final String admissionPosture;
  final String corridorStatus;
  final String publicReasonCode;
  final String sponsorDisplayState;

  factory RealmBootstrapView.fromJson(Map<String, dynamic> json) {
    return RealmBootstrapView(
      realmId: _stringField(json, 'realm_id'),
      slug: _stringField(json, 'slug'),
      displayName: _stringField(json, 'display_name'),
      realmStatus: _stringField(json, 'realm_status'),
      admissionPosture: _stringField(json, 'admission_posture'),
      corridorStatus: _stringField(json, 'corridor_status'),
      publicReasonCode: _stringField(json, 'public_reason_code'),
      sponsorDisplayState: _stringField(json, 'sponsor_display_state'),
    );
  }
}

class RealmAdmissionView {
  const RealmAdmissionView({
    required this.realmId,
    required this.accountId,
    required this.admissionStatus,
    required this.admissionKind,
    required this.publicReasonCode,
  });

  final String realmId;
  final String accountId;
  final String admissionStatus;
  final String admissionKind;
  final String publicReasonCode;

  factory RealmAdmissionView.fromJson(Map<String, dynamic> json) {
    return RealmAdmissionView(
      realmId: _stringField(json, 'realm_id'),
      accountId: _stringField(json, 'account_id'),
      admissionStatus: _stringField(json, 'admission_status'),
      admissionKind: _stringField(json, 'admission_kind'),
      publicReasonCode: _stringField(json, 'public_reason_code'),
    );
  }
}

class RealmBootstrapSummaryBundle {
  const RealmBootstrapSummaryBundle({
    required this.realmRequest,
    required this.bootstrapView,
    required this.admissionView,
  });

  final RealmRequestView? realmRequest;
  final RealmBootstrapView bootstrapView;
  final RealmAdmissionView? admissionView;

  factory RealmBootstrapSummaryBundle.fromJson(Map<String, dynamic> json) {
    final requestJson = json['realm_request'];
    final admissionJson = json['admission_view'];
    return RealmBootstrapSummaryBundle(
      realmRequest: requestJson is Map
          ? RealmRequestView.fromJson(requestJson.cast<String, dynamic>())
          : null,
      bootstrapView: RealmBootstrapView.fromJson(
        (json['bootstrap_view'] as Map).cast<String, dynamic>(),
      ),
      admissionView: admissionJson is Map
          ? RealmAdmissionView.fromJson(admissionJson.cast<String, dynamic>())
          : null,
    );
  }
}

String realmRequestStateLabel(String state) {
  return switch (state) {
    'requested' => '申請済み',
    'pending_review' => '確認中',
    'approved' => '承認済み',
    'rejected' => '見送り',
    _ => '確認中',
  };
}

String realmStatusLabel(String status) {
  return switch (status) {
    'limited_bootstrap' => '限定立ち上げ',
    'active' => '稼働中',
    'restricted' => '制限中',
    'suspended' => '停止中',
    'pending_review' => '確認中',
    _ => '確認中',
  };
}

String admissionPostureLabel(String posture) {
  return switch (posture) {
    'open' => '受付中',
    'limited' => '限定受付',
    'review_required' => '確認後に受付',
    'closed' => '受付停止',
    _ => '確認中',
  };
}

String admissionStatusLabel(String status) {
  return switch (status) {
    'pending' => '確認中',
    'admitted' => '参加中',
    'rejected' => '見送り',
    'revoked' => '取り消し',
    _ => '未申請',
  };
}

String admissionKindLabel(String kind) {
  return switch (kind) {
    'normal' => '通常',
    'sponsor_backed' => 'スポンサー経由',
    'corridor' => 'corridor',
    'review_required' => '確認待ち',
    _ => '未申請',
  };
}

String admissionQueueLabel(String status) {
  return switch (status) {
    'pending' => '確認キュー',
    'admitted' => '確定済み',
    'rejected' => '見送り済み',
    'revoked' => '取り消し済み',
    _ => '未申請',
  };
}

String corridorStatusLabel(String status) {
  return switch (status) {
    'active' => '有効',
    'cooling_down' => '冷却中',
    'expired' => '終了',
    'disabled_by_operator' => '停止中',
    'none' => 'なし',
    _ => '確認中',
  };
}

String sponsorDisplayStateLabel(String state) {
  return switch (state) {
    'sponsor_and_steward' => 'スポンサーとStewardあり',
    'sponsor_backed' => 'スポンサーあり',
    'steward_present' => 'Stewardあり',
    'none' => '未設定',
    _ => '確認中',
  };
}

String participantBootstrapCopy(RealmBootstrapSummaryBundle bundle) {
  final posture = bundle.bootstrapView.admissionPosture;
  final admissionStatus = bundle.admissionView?.admissionStatus;
  if (admissionStatus == 'admitted') {
    return '参加状態はwriter-ownedな記録から確認されています。';
  }
  if (posture == 'limited') {
    return '立ち上げ期間中です。受付は上限と確認状態に合わせて進みます。';
  }
  if (posture == 'review_required') {
    return '申請は確認を通して扱われます。参加はこの画面だけでは確定しません。';
  }
  if (posture == 'closed') {
    return '現在このRealmの新しい受付は止まっています。';
  }
  return 'Realmの状態を落ち着いて確認できます。';
}

String operatorBootstrapCopy(RealmBootstrapView view) {
  final parts = <String>[
    realmStatusLabel(view.realmStatus),
    admissionPostureLabel(view.admissionPosture),
    corridorStatusLabel(view.corridorStatus),
    sponsorDisplayStateLabel(view.sponsorDisplayState),
  ];
  return parts.join(' / ');
}

String _stringField(Map<String, dynamic> json, String key) {
  final value = json[key];
  if (value == null) {
    return '';
  }
  return '$value';
}

String? _nullableString(Map<String, dynamic> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  final normalized = '$value'.trim();
  return normalized.isEmpty ? null : normalized;
}

String? _trimmedOrNull(String? value) {
  final normalized = value?.trim();
  if (normalized == null || normalized.isEmpty) {
    return null;
  }
  return normalized;
}

String _summaryFromJson(Object? value) {
  if (value is Map) {
    final summary = value['summary'];
    if (summary != null && '$summary'.trim().isNotEmpty) {
      return '$summary';
    }
  }
  return '';
}
