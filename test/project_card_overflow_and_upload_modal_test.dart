import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atomusicator/core/di/desktop_studio_state.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:atomusicator/src/features/workbench/studio_panels.dart';
import 'package:atomusicator/src/features/workbench/upload_audio_dialog.dart';
import 'package:atomusicator/src/rust_bridge/api.dart' as native;

void main() {
  testWidgets(
    'Project card does not overflow 15 pixels when feedback banner is active and height is constrained',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 720);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final testProject = StudioProject(
        directory: 'C:/fake/projects/Proyecto_AppleCats_5Stems_20261109-120000',
        name: 'AppleCats - Algo Que Decirte.flac',
        duration: 245.0,
        sampleRate: 44100,
        channels: 2,
        stems: 5,
        modified: DateTime.now(),
      );

      final container = ProviderContainer(
        overrides: [
          projectsDirectoryProvider.overrideWith((ref) async => 'C:/fake/projects'),
          studioProjectsProvider.overrideWith((ref) async => [testProject]),
          favoriteProjectsProvider.overrideWith((ref) => {}),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(
              body: AudioWorkbench(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // No RenderFlex overflow exception should occur
      expect(tester.takeException(), isNull);
      expect(find.text('AppleCats - Algo Que Decirte.flac'), findsWidgets);
    },
  );

  testWidgets(
    'UploadAudioDialog renders file selection, folder choice, and preview name',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final container = ProviderContainer(
        overrides: [
          projectsDirectoryProvider.overrideWith((ref) async => 'C:/Users/Test/Atomusicator/projects'),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(
              body: UploadAudioDialog(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Subir Audio'), findsOneWidget);
      expect(find.text('Haga clic para seleccionar archivo de audio'), findsOneWidget);
      expect(find.text('Directorio por defecto (Recomendado)'), findsOneWidget);
      expect(find.text('Elegir directorio personalizado'), findsOneWidget);
      expect(find.text('C:/Users/Test/Atomusicator/projects'), findsOneWidget);
    },
  );

  testWidgets(
    'StudioFeedback deduplicates multiple interrupted jobs with same source file',
    (tester) async {
      final job1 = native.UiRecoverableJobDto(
        id: 'job-1',
        sourcePath: 'C:/audio/AppleCats - Algo Que Decirte.flac',
        message: 'interrupted',
      );
      final job2 = native.UiRecoverableJobDto(
        id: 'job-2',
        sourcePath: 'C:/audio/AppleCats - Algo Que Decirte.flac',
        message: 'interrupted',
      );

      final container = ProviderContainer(
        overrides: [
          recoverableJobsProvider.overrideWith((ref) async => [job1, job2]),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(
              body: StudioFeedback(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // Even with 2 raw jobs in list, only 1 unique row is displayed
      expect(
        find.text('Separación interrumpida: AppleCats - Algo Que Decirte.flac'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'Delete project dialog identifies disk directory and deletes on confirmation',
    (tester) async {
      tester.view.physicalSize = const Size(1400, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final tempDir = Directory.systemTemp.createTempSync('atomusicator_del_test_');
      addTearDown(() {
        if (tempDir.existsSync()) tempDir.deleteSync(recursive: true);
      });
      final projectDir = Directory('${tempDir.path}/Proyecto_Test_5Stems');
      projectDir.createSync(recursive: true);
      File('${projectDir.path}/session.json').writeAsStringSync('{"separation":{"status":"complete"}}');

      final testProject = StudioProject(
        directory: projectDir.path,
        name: 'Proyecto_Test.flac',
        duration: 120.0,
        sampleRate: 44100,
        channels: 2,
        stems: 5,
        modified: DateTime.now(),
      );

      final container = ProviderContainer(
        overrides: [
          projectsDirectoryProvider.overrideWith((ref) async => tempDir.path),
          studioProjectsProvider.overrideWith((ref) async => [testProject]),
          favoriteProjectsProvider.overrideWith((ref) => {projectDir.path}),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(
              body: AudioWorkbench(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // Find and tap the delete icon on the project card
      final deleteBtn = find.byTooltip('Eliminar proyecto').first;
      expect(deleteBtn, findsOneWidget);
      await tester.ensureVisible(deleteBtn);
      await tester.pumpAndSettle();
      await tester.tap(deleteBtn);
      await tester.pumpAndSettle();

      // Verify confirmation dialog displays identified folder path
      expect(find.text('¿Eliminar proyecto?'), findsOneWidget);
      expect(find.text('CARPETA EN DISCO IDENTIFICADA'), findsOneWidget);
      expect(find.text(projectDir.path), findsOneWidget);

      // Tap Confirm: "Eliminar proyecto y carpeta"
      await tester.runAsync(() async {
        await tester.tap(find.text('Eliminar proyecto y carpeta'));
        await Future<void>.delayed(const Duration(milliseconds: 150));
      });
      await tester.pumpAndSettle();

      // Verify physical directory was deleted from disk
      expect(projectDir.existsSync(), isFalse);
    },
  );
}
