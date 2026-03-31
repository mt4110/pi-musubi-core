import 'package:musubi_mobile/core/riverpod_compat.dart';

import 'pi_sdk_service_contract.dart';
import 'pi_sdk_service_stub.dart'
    if (dart.library.html) 'pi_sdk_service_web.dart'
    as impl;

export 'pi_sdk_service_contract.dart';

final piSdkServiceProvider = Provider<PiSdkService>((ref) {
  return impl.createPiSdkService();
});
