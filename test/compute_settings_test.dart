import 'dart:async';
import 'package:atomusicator/core/di/compute_settings.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:atomusicator/src/features/workbench/compute_settings_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

class FakeCompute implements ComputeSettingsGateway {
  String profile = 'balanced';
  bool fail = false;
  final saved = <String>[];
  @override
  Future<UiComputeSettingsDto> read() async => UiComputeSettingsDto(
    profile: profile,
    availableProfiles: ['eco', 'balanced', 'throughput'],
    requestedThreads: 6,
    effectiveThreads: 2,
  );
  @override
  Future<void> save(String value) async {
    if (fail) throw StateError('busy');
    saved.add(value);
    profile = value;
  }
}

class DelayedCancel extends FakeStudio {
  final acknowledgement = Completer<void>();
  int requests = 0;
  @override
  Future<void> cancel(String id) async {
    requests++;
    await acknowledgement.future;
    cancelled = true;
  }

  @override
  Future<UiJobDto> job(String id) async => UiJobDto(
    id: id,
    status: cancelled ? UiJobStatus.cancelled : UiJobStatus.cancelling,
    progress: .5,
    message: 'cleanup',
  );
}

void main() {
  ProviderContainer mount(FakeCompute compute, {FakeStudio? studio}) =>
      ProviderContainer(
        overrides: [
          computeSettingsGatewayProvider.overrideWithValue(compute),
          studioGatewayProvider.overrideWithValue(studio ?? FakeStudio()),
          nativeEngineStatusProvider.overrideWith(
            (ref) async => const NativeEngineStatus(
              engineVersion: 'test',
              applicationVersion: 'test',
              supportedStems: [],
            ),
          ),
        ],
      );

  test(
    'profiles persist through gateway and invalid or busy changes are rejected',
    () async {
      final fake = FakeCompute();
      final container = mount(fake);
      addTearDown(container.dispose);
      await container.read(computeSettingsProvider.future);
      for (final profile in ['eco', 'throughput', 'balanced']) {
        await container.read(computeSettingsProvider.notifier).select(profile);
        expect(
          container.read(computeSettingsProvider).requireValue.profile,
          profile,
        );
      }
      await container.read(computeSettingsProvider.notifier).select('invalid');
      expect(fake.saved.length, 3);
      await container.read(studioProvider.notifier).separate('/song.wav');
      await container.read(computeSettingsProvider.notifier).select('eco');
      expect(fake.saved.length, 3);
    },
  );

  testWidgets(
    'profile controls display actual effective threads and recover from error',
    (tester) async {
      final fake = FakeCompute();
      final container = mount(fake);
      addTearDown(() async {
        await tester.pumpWidget(const SizedBox.shrink());
        container.dispose();
      });
      await container.read(computeSettingsProvider.future);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(body: ComputeSettingsPanel()),
          ),
        ),
      );
      expect(find.textContaining('efectivos: 2'), findsOneWidget);
      fake.fail = true;
      await tester.tap(find.text('Ahorro'));
      await tester.pumpAndSettle();
      expect(find.text('Reintentar'), findsOneWidget);
      fake.fail = false;
      await tester.tap(find.text('Reintentar'));
      await tester.pumpAndSettle();
      expect(find.text('Equilibrado'), findsOneWidget);
    },
  );

  test(
    'cancel remains busy until rollback is acknowledged and rejects duplicate requests',
    () async {
      final fake = DelayedCancel();
      final container = mount(FakeCompute(), studio: fake);
      addTearDown(container.dispose);
      final controller = container.read(studioProvider.notifier);
      await controller.separate('/song.wav');
      final cancelling = controller.cancel();
      await controller.cancel();
      expect(container.read(studioProvider).cancelling, isTrue);
      expect(container.read(studioProvider).busy, isTrue);
      await Future<void>.delayed(Duration.zero);
      expect(fake.requests, 1);
      fake.acknowledgement.complete();
      await cancelling;
      expect(container.read(studioProvider).busy, isTrue);
      await controller.refresh();
      expect(container.read(studioProvider).busy, isFalse);
      expect(container.read(studioProvider).cancelling, isFalse);
      expect(container.read(studioProvider).session, isNull);
    },
  );
}
