class PiSdkUnavailableException implements Exception {
  const PiSdkUnavailableException([
    this.message = 'Pi SDK is unavailable in the current runtime.',
  ]);

  final String message;

  @override
  String toString() => message;
}

class PiSdkAuthPayload {
  const PiSdkAuthPayload({
    required this.uid,
    required this.username,
    required this.accessToken,
    this.walletAddress,
    this.profile = const <String, dynamic>{},
  });

  final String uid;
  final String username;
  final String accessToken;
  final String? walletAddress;
  final Map<String, dynamic> profile;
}

class PiSdkPaymentRequest {
  const PiSdkPaymentRequest({
    required this.amountPi,
    required this.memo,
    required this.recipientPiUid,
    this.metadata = const <String, dynamic>{},
  });

  final double amountPi;
  final String memo;
  final String recipientPiUid;
  final Map<String, dynamic> metadata;
}

class PiSdkPaymentResult {
  const PiSdkPaymentResult({
    required this.paymentId,
    required this.amountPi,
    required this.memo,
    required this.recipientPiUid,
    required this.status,
    this.txId,
    this.isStub = false,
  });

  final String paymentId;
  final double amountPi;
  final String memo;
  final String recipientPiUid;
  final String status;
  final String? txId;
  final bool isStub;
}

abstract class PiSdkService {
  bool get isAvailable;

  Future<PiSdkAuthPayload> authenticate({bool preferSilent = false});
  Future<PiSdkPaymentResult> createPayment(PiSdkPaymentRequest request);
}
