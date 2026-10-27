import 'dart:typed_data';
import 'package:file_picker/file_picker.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

class FiveStemStudio extends FakeStudio {
  @override
  UiSessionDto get session => UiSessionDto(
    id: 'five',
    directory: '/session',
    name: 'Luz de madrugada',
    pitchSemitones: 0,
    tracks: [
      for (final kind in stemNames.keys)
        UiStemTrackDto(
          stemType: kind,
          filePath: '/$kind.wav',
          sampleRate: 44100,
          channels: 2,
          durationSeconds: 203,
          peaks: Float32List.fromList(
            List.generate(800, (i) => i.isEven ? -0.4 : 0.6),
          ),
        ),
    ],
  );
}

class _ExportFolderPicker extends FilePicker {
  @override
  Future<String?> getDirectoryPath({
    String? dialogTitle,
    bool lockParentWindow = false,
    String? initialDirectory,
  }) async => '/export';
}

void main() {
  setUpAll(() => FilePicker.platform = _ExportFolderPicker());
  Future<ProviderContainer> mount(
    WidgetTester tester,
    FakeStudio gateway,
  ) async {
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
    await container.read(studioProvider.notifier).open('/session');
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(theme: AppTheme.dark, home: const AudioWorkbench()),
      ),
    );
    await tester.pump();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    });
    return container;
  }

  testWidgets(
    'DAW keeps track controls beside aligned waves and fixed on zoom',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final gateway = FiveStemStudio();
      final container = await mount(tester, gateway);
      final controls = find.byKey(const ValueKey('controls-lead_vocal'));
      final wave = find.byKey(const ValueKey('wave-lead_vocal'));
      expect(
        tester.getTopLeft(wave).dx,
        greaterThan(tester.getTopLeft(controls).dx),
      );
      expect(tester.getTopLeft(wave).dy, tester.getTopLeft(controls).dy);
      final before = tester.getTopLeft(controls);
      container.read(timelineZoomProvider.notifier).state = 4;
      await tester.pump();
      await tester.drag(
        find.byKey(const ValueKey('wave-scroll')),
        const Offset(-240, 0),
      );
      await tester.pump();
      expect(tester.getTopLeft(controls), before);
      await tester.tap(find.byTooltip('Mute Lead vocal'));
      await tester.pump();
      expect(gateway.calls, contains('track:lead_vocal:true:false'));
      expect(tester.takeException(), isNull);
      for (final size in [const Size(940, 650), const Size(1440, 900)]) {
        tester.view.physicalSize = size;
        await tester.pump();
        expect(tester.takeException(), isNull);
        expect(find.byTooltip('Play'), findsOneWidget);
      }
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    },
  );

  testWidgets('pitch dock applies selected semitones and preserves formants', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1440, 900);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final gateway = FiveStemStudio();
    final container = await mount(tester, gateway);
    await tester.tap(find.byTooltip('Raise pitch'));
    await tester.pump();
    await tester.tap(find.text('Apply pitch'));
    await tester.pump();
    expect(gateway.calls, contains('pitch:1:true'));
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  testWidgets('export dialog selects format and prevents exporting no tracks', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(940, 650);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final container = await mount(tester, FiveStemStudio());
    await tester.tap(find.byTooltip('Export stems').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('MP3'));
    await tester.pump();
    expect(find.text('320 kbps'), findsOneWidget);
    for (final key in stemNames.keys) {
      await tester.tap(find.byKey(ValueKey('export-$key')));
      await tester.pump();
    }
    expect(
      tester
          .widget<FilledButton>(find.byKey(const ValueKey('confirm-export')))
          .onPressed,
      isNull,
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  testWidgets('export excludes muted tracks only when requested', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(940, 650);
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final picker = FilePicker.platform;
    FilePicker.platform = _ExportFolderPicker();
    addTearDown(() => FilePicker.platform = picker);
    final gateway = FiveStemStudio();
    final container = await mount(tester, gateway);
    await container
        .read(studioProvider.notifier)
        .setTrack('bass', const MixTrackState(mute: true));
    await tester.tap(find.byTooltip('Export stems').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Include muted tracks'));
    await tester.pump();
    expect(find.text('4 OF 5 SELECTED'), findsOneWidget);
    await tester.ensureVisible(find.byKey(const ValueKey('confirm-export')));
    await tester.tap(find.byKey(const ValueKey('confirm-export')));
    await tester.pumpAndSettle();
    expect(
      gateway.calls,
      contains(
        'export:lead_vocal,backing_vocals,drums,instrumental:wav:44100:2:24:false',
      ),
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });
}
