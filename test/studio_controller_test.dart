import 'support/fake_studio.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late FakeStudio gateway;
  late ProviderContainer container;
  setUp(() {
    gateway = FakeStudio();
    container = ProviderContainer(
      overrides: [
        studioGatewayProvider.overrideWithValue(gateway),
        nativeEngineStatusProvider.overrideWith(
          (ref) async => const NativeEngineStatus(
            engineVersion: 'test',
            applicationVersion: 'test',
            supportedStems: [],
          ),
        ),
      ],
    );
  });
  tearDown(() => container.dispose());
  test(
    'separation publishes real session state and rejects overlapping UI jobs',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.separate('/song.wav');
      await controller.separate('/other.wav');
      expect(gateway.calls.where((c) => c.startsWith('separate:')), [
        'separate:/song.wav:false',
      ]);
      await controller.refresh();
      expect(container.read(studioProvider).session?.name, 'song.wav');
      expect(container.read(studioProvider).busy, false);
    },
  );
  test(
    'failed separation preserves loaded session and exposes the codec error',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      gateway.fail = true;
      await controller.separate('/broken.mp3');
      expect(container.read(studioProvider).error, contains('bad codec'));
      expect(container.read(studioProvider).session?.id, 'session');
      expect(container.read(studioProvider).busy, false);
    },
  );
  test(
    'cancellation clears busy state and mixer controls reach the gateway',
    () async {
      final controller = container.read(studioProvider.notifier);
      await controller.open('/session');
      await controller.setTrack(
        'bass',
        const MixTrackState(mute: true, solo: true),
      );
      await controller.setMaster(0.5);
      await controller.seek(2);
      await controller.transport('play');
      await controller.separate('/song.wav');
      await controller.cancel();
      await controller.refresh();
      expect(container.read(studioProvider).job?.status, 'cancelled');
      expect(container.read(studioProvider).busy, false);
      expect(
        gateway.calls,
        containsAll(['track:bass:true:true', 'master:0.5', 'seek:2.0', 'play']),
      );
    },
  );
}
