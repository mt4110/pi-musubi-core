import '../../features/realm_bootstrap/models/realm_bootstrap_models.dart';

abstract interface class RealmBootstrapRepository {
  Future<RealmRequestView> createRealmRequest(CreateRealmRequestDraft draft);

  Future<RealmRequestView> fetchRealmRequest(String realmRequestId);

  Future<RealmBootstrapSummaryBundle> fetchBootstrapSummary(String realmId);
}
