import 'package:atomusicator/core/theme/app_theme.dart';
import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:atomusicator/src/features/workbench/audio_workbench.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

const flacPath = r'C:\Music\AppleCats - Algo Que Decirte.FLAC';

class AudioPicker extends FilePicker {
  @override
  Future<FilePickerResult?> pickFiles({
    String? dialogTitle,
    String? initialDirectory,
    FileType type = FileType.any,
    List<String>? allowedExtensions,
    Function(FilePickerStatus)? onFileLoading,
    bool allowCompression = true,
    int compressionQuality = 30,
    bool allowMultiple = false,
    bool withData = false,
    bool withReadStream = false,
    bool lockParentWindow = false,
    bool readSequential = false,
  }) async {
    expect(type, FileType.custom);
    expect(allowedExtensions, containsAll(['wav', 'mp3', 'flac', 'm4a']));
    expect(withData, isFalse);
    return FilePickerResult([
      PlatformFile(
        path: flacPath,
        name: 'AppleCats - Algo Que Decirte.FLAC',
        size: 123,
      ),
    ]);
  }
}

void main() {
  Future<void> mount(
    WidgetTester tester,
    Future<void> Function(ProviderContainer) body,
  ) async {
    tester.view.physicalSize = const Size(1440, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final container = ProviderContainer(
      overrides: [
        nativeEngineStatusProvider.overrideWith(
          (ref) async => const NativeEngineStatus(
            engineVersion: 'test',
            applicationVersion: 'test',
            supportedStems: [],
          ),
        ),
      ],
    );
    try {
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
      await body(container);
    } finally {
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    }
  }

  testWidgets('picker includes FLAC and imports it without loading bytes', (
    tester,
  ) async {
    FilePicker.platform = AudioPicker();
    await mount(tester, (container) async {
      await tester.tap(find.text('Import audio'));
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(container.read(workbenchProvider).selectedSource?.path, flacPath);
    });
  });

  testWidgets('FLAC dropped over the header becomes the selected source', (
    tester,
  ) async {
    await mount(tester, (container) async {
      container.read(workbenchProvider.notifier).addSources([
        const SelectedAudioFile(
          path: '/old.wav',
          name: 'old.wav',
          sizeBytes: 1,
        ),
      ]);
      await tester.pump();
      final point = tester.getCenter(find.text('Import audio'));
      const codec = StandardMethodCodec();
      Future<void> send(String method, Object args) async {
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          'desktop_drop',
          codec.encodeMethodCall(MethodCall(method, args)),
          (_) {},
        );
        await tester.pump();
      }

      await send('entered', [point.dx, point.dy]);
      await send('performOperation', [flacPath]);
      expect(container.read(workbenchProvider).selectedSource?.path, flacPath);
    });
  });

  testWidgets('Ctrl+V imports Explorer file clipboard paths', (tester) async {
    const channel = MethodChannel('atomusicator/audio_import');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
      call,
    ) async {
      expect(call.method, 'readFilePaths');
      return [flacPath];
    });
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        channel,
        null,
      ),
    );
    await mount(tester, (container) async {
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump();
      expect(container.read(workbenchProvider).selectedSource?.path, flacPath);
    });
  });

  for (final scenario in ['empty', 'unsupported', 'busy']) {
    testWidgets('paste handles $scenario clipboard without changing source', (
      tester,
    ) async {
      const channel = MethodChannel('atomusicator/audio_import');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (
        _,
      ) async {
        if (scenario == 'busy') {
          throw PlatformException(
            code: 'clipboard_busy',
            message: 'Try again.',
          );
        }
        return scenario == 'empty' ? <String>[] : [r'C:\Music\document.pdf'];
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          channel,
          null,
        ),
      );
      await mount(tester, (container) async {
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pump();
        expect(container.read(workbenchProvider).sources, isEmpty);
        expect(find.byType(SnackBar), findsOneWidget);
        expect(tester.takeException(), isNull);
      });
    });
  }
}
