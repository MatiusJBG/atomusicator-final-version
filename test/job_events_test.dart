import 'dart:async';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

class EventStudio extends FakeStudio {
  final events = StreamController<UiJobDto>();
  int snapshots = 0;
  @override
  Stream<UiJobDto> jobChanges(UiJobDto previous) => events.stream;
  @override
  Future<UiJobDto> job(String id) async {
    snapshots++;
    return super.job(id);
  }
}

void main() {
  test(
    'native events finish jobs without status polling and disposal detaches listener',
    () async {
      final fake = EventStudio();
      final container = ProviderContainer(
        overrides: [
          studioGatewayProvider.overrideWithValue(fake),
          nativeEngineStatusProvider.overrideWith(
            (ref) async => const NativeEngineStatus(
              engineVersion: 'test',
              applicationVersion: 'test',
              supportedStems: [],
            ),
          ),
        ],
      );
      final controller = container.read(studioProvider.notifier);
      await controller.separate('/song.wav');
      fake.events.add(
        UiJobDto(
          id: 'job',
          status: UiJobStatus.running,
          progress: .5,
          message: 'stage',
        ),
      );
      await Future<void>.delayed(Duration.zero);
      expect(container.read(studioProvider).job!.progress, .5);
      fake.events.add(
        UiJobDto(
          id: 'job',
          status: UiJobStatus.completed,
          progress: 1,
          message: 'done',
          session: fake.session,
        ),
      );
      await Future<void>.delayed(Duration.zero);
      expect(container.read(studioProvider).busy, isFalse);
      expect(container.read(studioProvider).session!.id, fake.session.id);
      expect(fake.snapshots, 0);
      await controller.close();
      expect(container.read(studioProvider).session, isNull);
      expect(fake.calls, contains('close'));
      container.dispose();
      expect(fake.events.hasListener, isFalse);
      await fake.events.close();
    },
  );
}
