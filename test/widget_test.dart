import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'support/fake_studio.dart';

void main() {
  testWidgets('workbench opens with an empty audio session', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(theme: AppTheme.dark, home: const AudioWorkbench()),
      ),
    );

    await tester.tap(find.byTooltip('Atomizador'));
    await tester.pump();
    expect(find.text('Arrastra tu audio aquí'), findsOneWidget);
    expect(find.text('Importar audio'), findsOneWidget);
    expect(find.byType(StudioTransport), findsNothing);
  });

  testWidgets(
    'loaded workbench mixes, seeks and zooms without overflow at desktop sizes',
    (tester) async {
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
      var disposed = false;
      addTearDown(() {
        if (!disposed) container.dispose();
      });
      await container.read(studioProvider.notifier).open('/session');
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      tester.view.devicePixelRatio = 1;
      for (final size in [const Size(940, 650), const Size(1440, 900)]) {
        tester.view.physicalSize = size;
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
        expect(find.text('Bajo'), findsWidgets);
      }
      await tester.tap(find.text('M'));
      await tester.pump();
      expect(gateway.calls, contains('track:bass:true:false'));
      await tester.tap(find.byTooltip('Reproducir'));
      await tester.pump();
      expect(gateway.calls, contains('play'));
      container.read(timelineZoomProvider.notifier).state = 4;
      await tester.pump();
      expect(tester.takeException(), isNull);
      final waveform = find.byWidgetPredicate(
        (widget) => widget is CustomPaint && widget.painter is WaveformPainter,
      );
      await tester.tapAt(tester.getTopLeft(waveform) + const Offset(100, 20));
      await tester.pump();
      expect(gateway.calls.any((call) => call.startsWith('seek:')), isTrue);
      gateway.fail = true;
      gateway.failureMessage = List.filled(300, 'decode failure').join(' ');
      await container.read(studioProvider.notifier).separate('/bad.wav');
      await tester.pump();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
      disposed = true;
    },
  );
}
