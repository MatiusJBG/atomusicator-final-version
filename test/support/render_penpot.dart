// Render explicit test fixtures for visual comparison with Penpot:
// flutter test test/support/render_penpot.dart --update-goldens
import 'dart:math' as math;
import 'dart:typed_data';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'fake_studio.dart';

List<double> _fixturePeaks(int seed) => List.generate(1500, (i) {
  final x = (i ~/ 2) / 750;
  final envelope =
      x < .04 || (x > .27 && x < .31) || (x > .66 && x < .7)
          ? .01
          : (.18 + .45 * (math.sin(x * 24 + seed).abs())) *
              (.6 + .4 * math.sin(i * 31).abs());
  return i.isEven ? -envelope : envelope;
});

class _VisualStudio extends FakeStudio {
  bool processing = false;
  @override
  UiSessionDto get session => UiSessionDto(
    id: 'reference',
    directory: 'C:/Music/Sesión 04',
    name: 'mezcla_vocal.wav',
    pitchSemitones: 0,
    tracks: [
      for (var i = 0; i < stemNames.length; i++)
        UiStemTrackDto(
          stemType: stemNames.keys.elementAt(i),
          filePath: 'C:/Music/${stemNames.keys.elementAt(i)}.wav',
          sampleRate: 48000,
          channels: 2,
          durationSeconds: 203,
          peaks: Float32List.fromList(_fixturePeaks(i)),
        ),
    ],
  );
  @override
  Future<UiJobDto> job(String id) async =>
      processing
          ? UiJobDto(
            id: id,
            status: UiJobStatus.running,
            progress: .35,
            message: 'Separating vocal stems',
          )
          : super.job(id);
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    final font = FontLoader('Inter')
      ..addFont(rootBundle.load('assets/fonts/Inter.ttf'));
    await font.load();
    final icons = FontLoader('MaterialIcons')
      ..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'));
    await icons.load();
  });
  for (final screen in [
    'home',
    'empty',
    'ready',
    'processing',
    'result',
    'mixer',
    'export',
    'settings',
  ]) {
    testWidgets('Penpot visual fixture: $screen', (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetPhysicalSize);
      final gateway = _VisualStudio()..processing = screen == 'processing';
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
          projectsDirectoryProvider.overrideWith((ref) async => 'C:/Music'),
          studioProjectsProvider.overrideWith(
            (ref) async => [
              for (final name in [
                'Sesión 04 · mezcla_vocal',
                'Demo rock · take 2',
                'Podcast EP12',
              ])
                StudioProject(
                  directory: 'C:/Music/$name',
                  name: name,
                  duration: 203,
                  sampleRate: 48000,
                  channels: 2,
                  stems: 5,
                  modified: DateTime(2026, 10, 28),
                ),
            ],
          ),
          sourceAudioPreviewProvider.overrideWith(
            (ref, path) async => SourceAudioPreview(
              duration: 203,
              sampleRate: 48000,
              channels: 2,
              bitDepth: 24,
              peaks: _fixturePeaks(1),
            ),
          ),
        ],
      );
      if (['mixer', 'result', 'export'].contains(screen)) {
        await container
            .read(studioProvider.notifier)
            .open('C:/Music/Sesión 04');
      }
      if (['ready', 'processing'].contains(screen)) {
        container.read(workbenchProvider.notifier).addSources(const [
          SelectedAudioFile(
            path: 'C:/Music/mezcla_vocal.wav',
            name: 'mezcla_vocal.wav',
            sizeBytes: 35861299,
          ),
        ]);
      }
      container.read(studioViewProvider.notifier).state = switch (screen) {
        'home' => StudioView.home,
        'settings' => StudioView.settings,
        'mixer' || 'export' => StudioView.mixer,
        _ => StudioView.atomizer,
      };
      if (screen == 'result') {
        container.read(separationResultProvider.notifier).state = true;
      }
      const boundary = ValueKey('visual-canvas');
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: RepaintBoundary(
            key: boundary,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: AppTheme.dark,
              home: const AudioWorkbench(),
            ),
          ),
        ),
      );
      await tester.pump();
      if (screen == 'processing') {
        container.read(separationFlowProvider.notifier).state = true;
        await container
            .read(studioProvider.notifier)
            .separate('C:/Music/mezcla_vocal.wav');
        await container.read(studioProvider.notifier).refresh();
        await tester.pump();
      }
      if (screen == 'export') {
        await tester.tap(find.text('Exportar stems'));
        await tester.pumpAndSettle();
      }
      await tester.pump(const Duration(milliseconds: 100));
      expect(tester.takeException(), isNull);
      await expectLater(
        find.byKey(boundary),
        matchesGoldenFile('../../logs/penpot-$screen.png'),
      );
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    });
  }
}
