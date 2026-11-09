import 'dart:async';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/particle_loading_widgets.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_studio.dart';

class _ProgressStudio extends FakeStudio {
  final jobController = StreamController<UiJobDto>.broadcast();

  @override
  Stream<UiJobDto> jobChanges(UiJobDto previous) => jobController.stream;

  @override
  Future<UiJobDto> job(String id) async => UiJobDto(
        id: id,
        status: UiJobStatus.running,
        progress: 0.48,
        message: 'Aislando voz e instrumental',
      );

  @override
  UiSessionDto get session => UiSessionDto(
        id: 'pc-session',
        directory: '/pc-session',
        name: 'PC Master Track',
        pitchSemitones: 0,
        tracks: [
          for (final kind in stemNames.keys)
            UiStemTrackDto(
              stemType: kind,
              filePath: '/$kind.wav',
              sampleRate: 44100,
              channels: 2,
              durationSeconds: 200,
              peaks: Float32List.fromList(
                List.generate(200, (i) => i.isEven ? -0.4 : 0.6),
              ),
            ),
        ],
      );
}

void main() {
  testWidgets(
    'Local file upload and waveform analysis display ParticleWaveIntakeLoader with growing percentage on PC',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final previewCompleter = Completer<SourceAudioPreview?>();
      final container = ProviderContainer(
        overrides: [
          studioGatewayProvider.overrideWithValue(_ProgressStudio()),
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
          sourceAudioPreviewProvider.overrideWith(
            (ref, path) => previewCompleter.future,
          ),
        ],
      );

      container.read(studioViewProvider.notifier).state = StudioView.atomizer;
      // 1. Simulate active local file import at 35% -> 85%
      container.read(fileImportStateProvider.notifier).state = (
        importing: true,
        progress: 0.35,
        label: 'Cargando archivo local: cancion_estudio.wav',
      );

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
      expect(
        find.byKey(const ValueKey('local-file-import-particles')),
        findsOneWidget,
      );
      expect(find.text('35 %'), findsOneWidget);
      expect(
        find.text('Cargando archivo local: cancion_estudio.wav'),
        findsOneWidget,
      );

      // Advance import percentage to 85%
      container.read(fileImportStateProvider.notifier).state = (
        importing: true,
        progress: 0.85,
        label: 'Verificando cancion_estudio.wav…',
      );
      await tester.pump();
      expect(find.text('85 %'), findsOneWidget);

      // Finish file import and select the source so waveform preview decoding starts
      container.read(fileImportStateProvider.notifier).state = (
        importing: false,
        progress: 1.0,
        label: '',
      );
      container.read(workbenchProvider.notifier).addSources(const [
        SelectedAudioFile(
          path: 'C:/Music/cancion_estudio.wav',
          name: 'cancion_estudio.wav',
          sizeBytes: 32 * 1024 * 1024,
        ),
      ]);
      container
          .read(
            sourcePreviewProgressProvider(
              'C:/Music/cancion_estudio.wav',
            ).notifier,
          )
          .state = 0.62;
      await tester.pump();

      expect(tester.takeException(), isNull);
      expect(
        find.byKey(const ValueKey('waveform-preview-particles')),
        findsOneWidget,
      );
      expect(find.text('62 %'), findsOneWidget);

      previewCompleter.complete(null);
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
      debugDefaultTargetPlatformOverride = null;
    },
  );

  testWidgets(
    'Stem separation screen renders ParticleStemSplitterHero, ParticleProgressBar, and ParticleBurstLoader across percentages',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      final gateway = _ProgressStudio();
      final container = ProviderContainer(
        overrides: [
          studioGatewayProvider.overrideWithValue(gateway),
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

      container.read(workbenchProvider.notifier).addSources(const [
        SelectedAudioFile(
          path: 'C:/Music/mix.wav',
          name: 'mix.wav',
          sizeBytes: 20 * 1024 * 1024,
        ),
      ]);
      container.read(separationTargetModeProvider.notifier).state =
          SeparationTargetMode.twoStems;
      container.read(separationFlowProvider.notifier).state = true;
      container.read(studioViewProvider.notifier).state = StudioView.atomizer;
      await container
          .read(studioProvider.notifier)
          .separate('C:/Music/mix.wav', mode: 'two_stems');
      gateway.jobController.add(
        const UiJobDto(
          id: 'job',
          status: UiJobStatus.running,
          progress: 0.48,
          message: 'Aislando voz e instrumental',
        ),
      );
      await tester.pump();

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
      expect(find.byType(ParticleStemSplitterHero), findsOneWidget);
      expect(
        find.byKey(const ValueKey('separation-main-particle-bar')),
        findsOneWidget,
      );
      expect(find.byType(ParticleBurstLoader), findsWidgets);
      expect(find.text('48%'), findsOneWidget);
      expect(find.text('2 STEMS'), findsOneWidget);

      // Switch to 5 stems mode and verify 5 stem particle bars render cleanly
      container.read(separationTargetModeProvider.notifier).state =
          SeparationTargetMode.fiveStems;
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(find.text('5 STEMS'), findsOneWidget);
      for (final kind in stemNames.keys) {
        expect(
          find.byKey(ValueKey('stem-particle-bar-$kind')),
          findsOneWidget,
        );
      }

      await gateway.jobController.close();
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
      debugDefaultTargetPlatformOverride = null;
    },
  );

  testWidgets(
    'ParticleExportVisualizer renders in ExportDialog with stem particles',
    (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: Center(
              child: SizedBox(
                width: 480,
                child: ParticleExportVisualizer(
                  progress: 0.74,
                  selectedStems: ['lead_vocal', 'instrumental'],
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      expect(tester.takeException(), isNull);
      expect(find.byType(ParticleExportVisualizer), findsOneWidget);
      expect(find.byType(ParticleProgressBar), findsOneWidget);
    },
  );
}
