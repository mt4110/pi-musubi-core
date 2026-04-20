import '../../features/promise/models/promise_models.dart';

abstract class PromiseRepository {
  Future<CreatePromiseIntentResponse> createPromiseIntent(
    CreatePromiseIntentRequest request,
  );

  Future<PromiseStatusBundle> fetchPromiseStatus(
    String promiseIntentId, {
    String? settlementCaseId,
  });
}
