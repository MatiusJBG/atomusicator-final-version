import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

void main() {
  Future<ProviderContainer> mount(WidgetTester tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1440, 900);
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
    'Penpot navigation starts at Inicio and opens empty Atomizador without a mixer dock',
    (tester) async {
      final container = await mount(tester);
      expect(find.text('Atomusicator'), findsNWidgets(2));
      expect(find.text('Tus proyectos y mezclas recientes'), findsOneWidget);
      await tester.tap(find.byTooltip('Atomizador'));
      await tester.pump();
      expect(find.text('Arrastra tu audio aquí'), findsOneWidget);
      expect(find.text('Importa tu mezcla'), findsOneWidget);
      expect(find.byType(StudioTransport), findsNothing);
      expect(find.text('Not separated'), findsNothing);
      await tester.tap(find.byTooltip('Inicio'));
      await tester.pump();
      expect(find.text('Tus proyectos y mezclas recientes'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    },
  );

  testWidgets('an imported source opens the review screen before separation', (
    tester,
  ) async {
    final container = await mount(tester);
    container.read(workbenchProvider.notifier).addSources(const [
      SelectedAudioFile(
        path: '/reference.wav',
        name: 'reference.wav',
        sizeBytes: 2048,
      ),
    ]);
    await tester.pump();
    expect(find.text('STEMS A GENERAR'), findsOneWidget);
    expect(find.text('RESUMEN'), findsOneWidget);
    expect(find.text('Separar 5 stems'), findsOneWidget);
    expect(find.byType(StudioTransport), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  testWidgets('settings follows the two card layout from Penpot', (
    tester,
  ) async {
    final container = await mount(tester);
    await tester.tap(find.byTooltip('Ajustes'));
    await tester.pump();
    expect(find.text('Apariencia'), findsOneWidget);
    expect(find.text('Motor e IA'), findsOneWidget);
    expect(find.byType(StudioTransport), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  testWidgets('separation shows its result before opening the mixer', (
    tester,
  ) async {
    final container = await mount(tester);
    container.read(workbenchProvider.notifier).addSources(const [
      SelectedAudioFile(
        path: '/reference.wav',
        name: 'reference.wav',
        sizeBytes: 2048,
      ),
    ]);
    await tester.pump();
    await tester.tap(find.text('Separar 5 stems'));
    await tester.pump();
    expect(find.text('ETAPAS DEL PROCESO'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: 'processing layout');
    expect(find.byType(StudioTransport), findsNothing);
    await container.read(studioProvider.notifier).refresh();
    await tester.pump();
    expect(find.text('Tus 1 stems están listos'), findsOneWidget);
    expect(tester.takeException(), isNull, reason: 'result layout');
    expect(find.byType(StudioTransport), findsNothing);
    await tester.tap(find.text('Abrir mezclador'));
    await tester.pump();
    expect(find.byType(StudioTransport), findsOneWidget);
    expect(tester.takeException(), isNull, reason: 'mixer layout');
    await tester.tap(find.text('Nuevo proyecto'));
    await tester.pump();
    expect(find.text('Arrastra tu audio aquí'), findsOneWidget);
    expect(container.read(workbenchProvider).sources, isEmpty);
    expect(container.read(studioProvider).session, isNotNull);
    expect(container.read(studioViewProvider), StudioView.atomizer);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
    container.dispose();
  });

  testWidgets(
    'Penpot routes fit the minimum desktop window with enlarged text',
    (tester) async {
      final container = await mount(tester);
      tester.view.physicalSize = const Size(920, 640);
      container.read(studioTextScaleProvider.notifier).state = 1.25;
      for (final view in [
        StudioView.home,
        StudioView.atomizer,
        StudioView.settings,
      ]) {
        container.read(studioViewProvider.notifier).state = view;
        await tester.pump();
        expect(tester.takeException(), isNull, reason: '$view at 920x640');
      }
      container.read(workbenchProvider.notifier).addSources(const [
        SelectedAudioFile(
          path: '/reference.wav',
          name: 'reference.wav',
          sizeBytes: 2048,
        ),
      ]);
      await tester.pump();
      expect(tester.takeException(), isNull, reason: 'ready at 920x640');
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    },
  );
}
