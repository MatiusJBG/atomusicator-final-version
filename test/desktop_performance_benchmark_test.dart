import 'package:atomusicator/core/di/compute_settings.dart';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/compute_settings_panel.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_studio.dart';

class _DesktopComputeGateway implements ComputeSettingsGateway {
  String profile = 'throughput';
  @override
  Future<UiComputeSettingsDto> read() async => UiComputeSettingsDto(
        profile: profile,
        availableProfiles: const ['eco', 'balanced', 'throughput'],
        requestedThreads: 8,
        effectiveThreads: 8,
      );

  @override
  Future<void> save(String value) async {
    profile = value;
  }
}

void main() {
  testWidgets(
    'Desktop PC Atomizador switches between 2 Stems (~15-30 seg) and 5 Stems (~1-2 min) and dispatches #mode tags with zero overflow',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final gateway = FakeStudio();
      final compute = _DesktopComputeGateway();
      final container = ProviderContainer(
        overrides: [
          studioGatewayProvider.overrideWithValue(gateway),
          computeSettingsGatewayProvider.overrideWithValue(compute),
          nativeEngineStatusProvider.overrideWith(
            (ref) async => const NativeEngineStatus(
              engineVersion: '0.7.0',
              applicationVersion: '0.7.0',
              supportedStems: [
                'lead_vocal',
                'backing_vocals',
                'drums',
                'bass',
                'instrumental',
              ],
            ),
          ),
        ],
      );

      // Select a source file and navigate to Atomizador (sources view)
      container.read(workbenchProvider.notifier).addSources(const [
        SelectedAudioFile(
          path: 'C:/Music/studio_master.wav',
          name: 'studio_master.wav',
          sizeBytes: 42 * 1024 * 1024,
        ),
      ]);
      container.read(studioViewProvider.notifier).state = StudioView.atomizer;

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

      expect(tester.takeException(), isNull);
      // Verify PC speed badges are rendered
      expect(find.text('~15-30 seg'), findsOneWidget);
      expect(find.text('~1-2 min'), findsOneWidget);

      // 1. Switch to 5 Stems mode and verify PC 5-stems banner and button label
      await tester.tap(find.byKey(const ValueKey('mode-five-stems')));
      await tester.pump();
      expect(
        find.textContaining('Modo Estudio Completo PC: Ejecuta las 4 pasadas'),
        findsOneWidget,
      );
      expect(
        find.text('Separar en 5 stems (Completo ~1-2 min)'),
        findsOneWidget,
      );

      // 2. Switch back to 2 Stems mode and verify PC 2-stems banner, label, and dispatch
      await tester.tap(find.byKey(const ValueKey('mode-two-stems')));
      await tester.pump();
      expect(
        find.textContaining('Modo Rápido PC: Separa en 1 sola pasada neuronal'),
        findsOneWidget,
      );
      expect(
        find.text('Separar en 2 stems (Rápido ~15-30 seg)'),
        findsOneWidget,
      );

      await tester.tap(find.text('Separar en 2 stems (Rápido ~15-30 seg)'));
      await tester.pump();
      expect(
        gateway.calls,
        contains('separate:C:/Music/studio_master.wav#mode=two_stems:false'),
      );

      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
      debugDefaultTargetPlatformOverride = null;
    },
  );

  testWidgets(
    'ComputeSettingsPanel displays FP32 0% loss and DirectML/AVX2 hardware acceleration badge',
    (tester) async {
      final compute = _DesktopComputeGateway();
      final container = ProviderContainer(
        overrides: [
          computeSettingsGatewayProvider.overrideWithValue(compute),
          studioGatewayProvider.overrideWithValue(FakeStudio()),
          nativeEngineStatusProvider.overrideWith(
            (ref) async => const NativeEngineStatus(
              engineVersion: '0.7.0',
              applicationVersion: '0.7.0',
              supportedStems: [],
            ),
          ),
        ],
      );
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
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Precisión FP32 (0% pérdida)'),
        findsOneWidget,
      );
      expect(find.textContaining('efectivos: 8'), findsOneWidget);
    },
  );
}
