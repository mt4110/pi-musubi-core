class CreatePromiseIntentRequest {
  const CreatePromiseIntentRequest({
    required this.internalIdempotencyKey,
    required this.realmId,
    required this.counterpartyAccountId,
    required this.depositAmountMinorUnits,
    required this.currencyCode,
  });

  final String internalIdempotencyKey;
  final String realmId;
  final String counterpartyAccountId;
  final int depositAmountMinorUnits;
  final String currencyCode;

  Map<String, Object?> toJson() {
    return {
      'internal_idempotency_key': internalIdempotencyKey,
      'realm_id': realmId,
      'counterparty_account_id': counterpartyAccountId,
      'deposit_amount_minor_units': depositAmountMinorUnits,
      'currency_code': currencyCode,
    };
  }
}

class CreatePromiseIntentResponse {
  const CreatePromiseIntentResponse({
    required this.promiseIntentId,
    required this.settlementCaseId,
    required this.caseStatus,
    required this.replayedIntent,
  });

  final String promiseIntentId;
  final String settlementCaseId;
  final String caseStatus;
  final bool replayedIntent;

  factory CreatePromiseIntentResponse.fromJson(Map<String, dynamic> json) {
    return CreatePromiseIntentResponse(
      promiseIntentId: _stringField(json, 'promise_intent_id'),
      settlementCaseId: _stringField(json, 'settlement_case_id'),
      caseStatus: _stringField(json, 'case_status'),
      replayedIntent: json['replayed_intent'] == true,
    );
  }
}

class PromiseProjectionView {
  const PromiseProjectionView({
    required this.promiseIntentId,
    required this.realmId,
    required this.initiatorAccountId,
    required this.counterpartyAccountId,
    required this.currentIntentStatus,
    required this.depositAmountMinorUnits,
    required this.currencyCode,
    required this.depositScale,
    required this.latestSettlementCaseId,
    required this.latestSettlementStatus,
  });

  final String promiseIntentId;
  final String realmId;
  final String initiatorAccountId;
  final String counterpartyAccountId;
  final String currentIntentStatus;
  final int depositAmountMinorUnits;
  final String currencyCode;
  final int depositScale;
  final String? latestSettlementCaseId;
  final String? latestSettlementStatus;

  factory PromiseProjectionView.fromJson(Map<String, dynamic> json) {
    return PromiseProjectionView(
      promiseIntentId: _stringField(json, 'promise_intent_id'),
      realmId: _stringField(json, 'realm_id'),
      initiatorAccountId: _stringField(json, 'initiator_account_id'),
      counterpartyAccountId: _stringField(json, 'counterparty_account_id'),
      currentIntentStatus: _stringField(json, 'current_intent_status'),
      depositAmountMinorUnits: _intField(json, 'deposit_amount_minor_units'),
      currencyCode: _stringField(json, 'currency_code'),
      depositScale: _intField(json, 'deposit_scale'),
      latestSettlementCaseId:
          _nullableString(json, 'latest_settlement_case_id'),
      latestSettlementStatus: _nullableString(json, 'latest_settlement_status'),
    );
  }
}

class ExpandedSettlementView {
  const ExpandedSettlementView({
    required this.settlementCaseId,
    required this.promiseIntentId,
    required this.realmId,
    required this.currentSettlementStatus,
    required this.totalFundedMinorUnits,
    required this.currencyCode,
    required this.proofStatus,
    required this.proofSignalCount,
  });

  final String settlementCaseId;
  final String promiseIntentId;
  final String realmId;
  final String currentSettlementStatus;
  final int totalFundedMinorUnits;
  final String currencyCode;
  final String proofStatus;
  final int proofSignalCount;

  factory ExpandedSettlementView.fromJson(Map<String, dynamic> json) {
    return ExpandedSettlementView(
      settlementCaseId: _stringField(json, 'settlement_case_id'),
      promiseIntentId: _stringField(json, 'promise_intent_id'),
      realmId: _stringField(json, 'realm_id'),
      currentSettlementStatus: _stringField(
        json,
        'current_settlement_status',
      ),
      totalFundedMinorUnits: _intField(json, 'total_funded_minor_units'),
      currencyCode: _stringField(json, 'currency_code'),
      proofStatus: _stringField(json, 'proof_status'),
      proofSignalCount: _intField(json, 'proof_signal_count'),
    );
  }
}

class PromiseStatusBundle {
  const PromiseStatusBundle({
    required this.promiseIntentId,
    required this.initialSettlementCaseId,
    required this.promise,
    required this.settlement,
  });

  final String promiseIntentId;
  final String? initialSettlementCaseId;
  final PromiseProjectionView? promise;
  final ExpandedSettlementView? settlement;

  String? get settlementCaseId {
    return settlement?.settlementCaseId ??
        promise?.latestSettlementCaseId ??
        initialSettlementCaseId;
  }

  String get promiseStatus {
    return promise?.currentIntentStatus ?? 'pending_projection';
  }

  String get settlementStatus {
    return settlement?.currentSettlementStatus ??
        promise?.latestSettlementStatus ??
        'pending_projection';
  }

  String get proofStatus {
    return settlement?.proofStatus ?? 'unavailable';
  }

  bool get hasParticipantSafeProjection {
    return promise != null || settlement != null;
  }
}

String promiseStatusLabel(String status) {
  return switch (status) {
    'proposed' => '提案中',
    'confirmed' => '約束済み',
    'fulfilled' => '完了',
    'reflected' => 'ふりかえり済み',
    'withdrawn' => '取り下げ',
    'under_review' => '確認中',
    'pending_projection' => '表示準備中',
    _ => '確認中',
  };
}

String settlementStatusLabel(String status) {
  return switch (status) {
    'pending_funding' => 'デポジット準備中',
    'hold_opened' => 'デポジット手続き中',
    'funded' => 'デポジット確認済み',
    'manual_review' => '確認中',
    'pending_projection' => '表示準備中',
    _ => '確認中',
  };
}

String proofStatusLabel(String status) {
  return switch (status) {
    'verified' => '証明確認済み',
    'missing' => '追加確認が必要',
    'quarantined' => '確認中',
    'manual_review' => '確認中',
    'unavailable' => '証明機能は準備中',
    _ => '証明待ち',
  };
}

String participantNextActionCopy(PromiseStatusBundle bundle) {
  if (!bundle.hasParticipantSafeProjection) {
    return '作成は受け付けました。表示の準備が整うまで少し待ってください。';
  }
  if (bundle.proofStatus == 'unavailable') {
    return '完了はまだこの画面だけでは確定しません。証明機能が使える状態になるまで、約束の状態をここで確認できます。';
  }
  if (bundle.settlementStatus == 'pending_funding') {
    return 'デポジットの確認を待っています。相手へのアクセス権ではなく、約束を丁寧に扱うための預かりです。';
  }
  if (bundle.proofStatus == 'missing' || bundle.proofStatus == 'quarantined') {
    return 'すぐに相手を責めず、追加確認を待ちます。必要な場合は確認フローにつながります。';
  }
  return '次の案内が出るまで、この約束の状態を確認できます。';
}

String _stringField(Map<String, dynamic> json, String key) {
  final value = json[key];
  if (value is String && value.isNotEmpty) {
    return value;
  }
  throw FormatException('Missing string field: $key');
}

String? _nullableString(Map<String, dynamic> json, String key) {
  final value = json[key];
  if (value == null) {
    return null;
  }
  if (value is String && value.isNotEmpty) {
    return value;
  }
  return null;
}

int _intField(Map<String, dynamic> json, String key) {
  final value = json[key];
  if (value is int) {
    return value;
  }
  if (value is num) {
    return value.toInt();
  }
  if (value is String) {
    return int.parse(value);
  }
  throw FormatException('Missing integer field: $key');
}
