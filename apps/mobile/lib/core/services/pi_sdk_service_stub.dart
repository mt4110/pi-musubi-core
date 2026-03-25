import 'pi_sdk_service_contract.dart';

PiSdkService createPiSdkService() => const _UnavailablePiSdkService();

class _UnavailablePiSdkService implements PiSdkService {
  const _UnavailablePiSdkService();

  @override
  bool get isAvailable => false;

  @override
  Future<PiSdkAuthPayload> authenticate({bool preferSilent = false}) {
    throw const PiSdkUnavailableException();
  }

  @override
  Future<PiSdkPaymentResult> createPayment(PiSdkPaymentRequest request) async {
    await Future<void>.delayed(const Duration(milliseconds: 900));
    final stamp = DateTime.now().millisecondsSinceEpoch;
    return PiSdkPaymentResult(
      paymentId: 'pi-stub-payment-$stamp',
      amountPi: request.amountPi,
      memo: request.memo,
      recipientPiUid: request.recipientPiUid,
      status: 'stubbed',
      txId: 'pi-stub-tx-$stamp',
      isStub: true,
    );
  }
}
