import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_studio.dart';

void main() {
  testWidgets(
    'source list selects one file and updates the separation target',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = FakeStudio();
      final container = ProviderContainer(
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
      container.read(workbenchProvider.notifier).addSources(const [
        SelectedAudioFile(
          path: '/first.wav',
          name: 'first.wav',
          sizeBytes: 1024,
        ),
        SelectedAudioFile(
          path: '/second.flac',
          name: 'second.flac',
          sizeBytes: 2048,
        ),
      ]);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: MaterialApp(
            theme: AppTheme.dark,
            home: const AudioWorkbench(),
          ),
        ),
      );
      await tester.pump();
      expect(
        tester
            .widget<InputChip>(find.byKey(const ValueKey('source-/first.wav')))
            .selected,
        isTrue,
      );
      await tester.tap(find.byKey(const ValueKey('source-/second.flac')));
      await tester.pump();
      expect(
        container.read(workbenchProvider).selectedSource?.path,
        '/second.flac',
      );
      expect(
        tester
            .widget<InputChip>(
              find.byKey(const ValueKey('source-/second.flac')),
            )
            .selected,
        isTrue,
      );
      await tester.ensureVisible(find.text('Separar 5 stems'));
      await tester.tap(find.text('Separar 5 stems'));
      await tester.pump();
      expect(gateway.calls, contains('separate:/second.flac:false'));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    },
  );

  testWidgets('empty studio adapts to desktop windows and larger text', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final container = ProviderContainer(
      overrides: [
        studioGatewayProvider.overrideWithValue(FakeStudio()),
        nativeEngineStatusProvider.overrideWith(
          (ref) async => const NativeEngineStatus(
            engineVersion: 'test',
            applicationVersion: 'test',
            supportedStems: [],
          ),
        ),
      ],
    );
    for (final size in [
      const Size(920, 640),
      const Size(1200, 720),
      const Size(1440, 900),
      const Size(1920, 1080),
    ]) {
      tester.view.physicalSize = size;
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: MaterialApp(
            theme: AppTheme.dark,
            builder:
                (context, child) => MediaQuery(
                  data: MediaQuery.of(
                    context,
                  ).copyWith(textScaler: TextScaler.linear(1.25)),
                  child: child!,
                ),
            home: const AudioWorkbench(),
          ),
        ),
      );
      await tester.pump();
      await tester.tap(find.byTooltip('Atomizador'));
      await tester.pump();
      expect(find.text('Arrastra tu audio aquí'), findsOneWidget);
      expect(tester.takeException(), isNull, reason: 'Window: $size');
    }
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });
}
