import 'dart:convert';
import 'dart:io';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'project library reads complete local sessions and skips corrupt metadata',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'atomusicator-library-',
      );
      addTearDown(() => root.delete(recursive: true));
      for (final name in ['old', 'new', 'partial', 'corrupt']) {
        final dir = await Directory('${root.path}/$name').create();
        final file = File('${dir.path}/session.json');
        await file.writeAsString(
          name == 'corrupt'
              ? '{'
              : jsonEncode({
                'file_name': name,
                'duration_seconds': 120.5,
                'sample_rate_hz': 48000,
                'channels': 2,
                'separation': {
                  'status': name == 'partial' ? 'running' : 'complete',
                  'stems': [{}, {}, {}, {}, {}],
                },
              }),
        );
        await file.setLastModified(DateTime(2026, 10, name == 'new' ? 28 : 27));
      }
      final container = ProviderContainer(
        overrides: [
          projectsDirectoryProvider.overrideWith((ref) async => root.path),
        ],
      );
      addTearDown(container.dispose);
      final projects = await container.read(studioProjectsProvider.future);
      expect(projects.map((p) => p.name), ['new', 'old']);
      expect(projects.first.stems, 5);
      expect(projects.first.duration, 120.5);
      expect(projects.first.sampleRate, 48000);
    },
  );

  testWidgets(
    'saved desktop preferences restore text scale, density and grid',
    (tester) async {
      final root = await tester.runAsync(
        () => Directory.systemTemp.createTemp('atomusicator-preferences-'),
      );
      final file = File('${root!.path}/desktop-ui.json');
      final container = ProviderContainer(
        overrides: [desktopPreferencesFileProvider.overrideWithValue(file)],
      );
      late WidgetRef widgetRef;
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: Consumer(
            builder: (context, ref, _) {
              widgetRef = ref;
              return const SizedBox.shrink();
            },
          ),
        ),
      );
      container.read(studioTextScaleProvider.notifier).state = 1.2;
      container.read(studioDensityProvider.notifier).state = .9;
      container.read(studioGridProvider.notifier).state = false;
      container.read(favoriteProjectsProvider.notifier).state = {
        'C:/Music/session',
      };
      await tester.runAsync(() => saveDesktopPreferences(widgetRef));
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
      final restored = ProviderContainer(
        overrides: [desktopPreferencesFileProvider.overrideWithValue(file)],
      );
      await tester.runAsync(
        () => restored.read(desktopPreferencesLoadProvider.future),
      );
      expect(restored.read(studioTextScaleProvider), 1.2);
      expect(restored.read(studioDensityProvider), .9);
      expect(restored.read(studioGridProvider), isFalse);
      expect(restored.read(favoriteProjectsProvider), {'C:/Music/session'});
      restored.dispose();
      await tester.runAsync(() => root.delete(recursive: true));
    },
  );

  final decoder = Platform.environment['ATOMUSICATOR_FFMPEG'];
  test(
    'native audio tools generate bounded waveform preview from real WAV',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'atomusicator-preview-',
      );
      addTearDown(() => root.delete(recursive: true));
      final path = '${root.path}/source.wav';
      final generated = await Process.run(decoder!, [
        '-v',
        'error',
        '-f',
        'lavfi',
        '-i',
        'sine=frequency=440:sample_rate=8000:duration=0.2',
        '-c:a',
        'pcm_s16le',
        path,
      ]);
      expect(generated.exitCode, 0, reason: '${generated.stderr}');
      final container = ProviderContainer(
        overrides: [
          previewAudioToolsDirectoryProvider.overrideWithValue(
            File(decoder).parent.path,
          ),
        ],
      );
      final provider = sourceAudioPreviewProvider(path);
      final subscription = container.listen(provider, (_, __) {});
      addTearDown(() {
        subscription.close();
        container.dispose();
      });
      final preview = (await container.read(provider.future))!;
      expect(preview.duration, closeTo(.2, .01));
      expect(preview.sampleRate, 8000);
      expect(preview.channels, 1);
      expect(preview.bitDepth, 16);
      expect(preview.peaks.length, 1300);
      expect(preview.peaks.any((p) => p > 0), isTrue);
      expect(preview.peaks.any((p) => p < 0), isTrue);
      expect(preview.peaks.skip(1200).any((p) => p != 0), isTrue);
    },
    skip:
        decoder == null
            ? 'Set ATOMUSICATOR_FFMPEG to test the bundled audio tools.'
            : false,
  );
}
