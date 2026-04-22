import '../../api/api_client.dart';
import '../../features/realm_bootstrap/models/realm_bootstrap_models.dart';
import 'realm_bootstrap_repository.dart';

class ApiRealmBootstrapRepository implements RealmBootstrapRepository {
  ApiRealmBootstrapRepository(this._apiClient);

  final ApiClient _apiClient;

  @override
  Future<RealmRequestView> createRealmRequest(
    CreateRealmRequestDraft draft,
  ) async {
    final response = await _apiClient.dio.post<Map<String, dynamic>>(
      '/api/realms/requests',
      data: draft.toJson(),
    );
    return RealmRequestView.fromJson(
      response.data ?? const <String, dynamic>{},
    );
  }

  @override
  Future<RealmRequestView> fetchRealmRequest(String realmRequestId) async {
    final response = await _apiClient.dio.get<Map<String, dynamic>>(
      '/api/realms/requests/${Uri.encodeComponent(realmRequestId)}',
    );
    return RealmRequestView.fromJson(
      response.data ?? const <String, dynamic>{},
    );
  }

  @override
  Future<RealmBootstrapSummaryBundle> fetchBootstrapSummary(
    String realmId,
  ) async {
    final response = await _apiClient.dio.get<Map<String, dynamic>>(
      '/api/projection/realms/${Uri.encodeComponent(realmId)}/bootstrap-summary',
    );
    return RealmBootstrapSummaryBundle.fromJson(
      response.data ?? const <String, dynamic>{},
    );
  }
}
