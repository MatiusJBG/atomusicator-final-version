import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../src/rust_bridge/api.dart' as native;
import 'native_engine_provider.dart';
import 'studio_controller.dart';

abstract class ComputeSettingsGateway {
  Future<native.UiComputeSettingsDto> read();
  Future<void> save(String profile);
}

class NativeComputeSettingsGateway implements ComputeSettingsGateway {
  @override
  Future<native.UiComputeSettingsDto> read() => native.computeSettings();
  @override
  Future<void> save(String profile) =>
      native.setComputeProfile(profile: profile);
}

final computeSettingsGatewayProvider = Provider<ComputeSettingsGateway>(
  (ref) => NativeComputeSettingsGateway(),
);
final computeSettingsProvider = AsyncNotifierProvider<
  ComputeSettingsController,
  native.UiComputeSettingsDto
>(ComputeSettingsController.new);

class ComputeSettingsController
    extends AsyncNotifier<native.UiComputeSettingsDto> {
  bool _disposed = false;
  @override
  Future<native.UiComputeSettingsDto> build() async {
    _disposed = false;
    ref.onDispose(() => _disposed = true);
    await ref.watch(nativeEngineStatusProvider.future);
    return ref.read(computeSettingsGatewayProvider).read();
  }

  Future<void> select(String profile) async {
    if (state.isLoading || ref.read(studioProvider).busy) return;
    final previous = state.valueOrNull;
    if (previous == null || !previous.availableProfiles.contains(profile)) {
      return;
    }
    state = const AsyncLoading();
    final gateway = ref.read(computeSettingsGatewayProvider);
    final result = await AsyncValue.guard(() async {
      await gateway.save(profile);
      return gateway.read();
    });
    if (!_disposed) state = result;
  }
}
