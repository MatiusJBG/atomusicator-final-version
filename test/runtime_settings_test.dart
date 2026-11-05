import 'package:atomusicator/core/di/native_engine_provider.dart';
import 'package:atomusicator/core/di/studio_controller.dart';
import 'package:atomusicator/src/features/workbench/execution_settings_panel.dart';
import 'package:atomusicator/src/features/workbench/cache_settings_panel.dart';
import 'package:atomusicator/src/rust_bridge/api.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_studio.dart';

class FakeExecution implements ExecutionSettingsGateway {
  String mode = 'auto';
  final saved = <String>[];
  @override
  Future<UiExecutionSettingsDto> read() async => UiExecutionSettingsDto(
    nextStartupMode: mode,
    diagnosticsJson:
        '[{"provider_registered":"Cpu","provider_effective":"CPUExecutionProvider","fallback_errors":["GPU preflight rejected"]}]',
  );
  @override
  Future<void> save(String value) async {
    saved.add(value);
    mode = value;
  }
}

class FakeCache implements CacheSettingsGateway {
  BigInt bytes = BigInt.from(4 * 1073741824);
  int cleaned = 0;
  bool fail = false;
  @override
  Future<UiCacheSettingsDto> read() async => UiCacheSettingsDto(
    retainedBytes: bytes,
    usedBytes: BigInt.from(1073741824),
    directory: '/cache',
  );
  @override
  Future<void> save(BigInt value) async {
    bytes = value;
  }

  @override
  Future<BigInt> clean() async {
    if (fail) throw StateError('disk error');
    cleaned++;
    return BigInt.from(1073741824);
  }
}

void main() {
  testWidgets('provider preference explains restart and actual CPU fallback', (
    tester,
  ) async {
    final fake = FakeExecution();
    final container = ProviderContainer(
      overrides: [
        executionGatewayProvider.overrideWithValue(fake),
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
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      container.dispose();
    });
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: ExecutionSettingsPanel()),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('GPU preflight rejected'), findsOneWidget);
    expect(find.textContaining('Se aplica al reiniciar'), findsOneWidget);
    await tester.tap(find.byType(DropdownButton<String>));
    await tester.pumpAndSettle();
    await tester.tap(find.text('CPU · máxima compatibilidad').last);
    await tester.pumpAndSettle();
    expect(fake.saved, ['cpu']);
  });

  testWidgets(
    'cache cleanup reports reclaimed bytes and rejects cleanup while busy',
    (tester) async {
      final fake = FakeCache();
      final container = ProviderContainer(
        overrides: [
          cacheGatewayProvider.overrideWithValue(fake),
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
      addTearDown(() async {
        await tester.pumpWidget(const SizedBox.shrink());
        container.dispose();
      });
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(home: Scaffold(body: CacheSettingsPanel())),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Limpiar caché sin uso'));
      await tester.pumpAndSettle();
      expect(fake.cleaned, 1);
      expect(find.text('Liberados 1.00 GiB.'), findsOneWidget);
      fake.fail = true;
      await tester.tap(find.text('Limpiar caché sin uso'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Comprueba el espacio'), findsOneWidget);
      await container.read(studioProvider.notifier).separate('/song.wav');
      await tester.pump();
      final button = tester.widget<TextButton>(
        find.widgetWithText(TextButton, 'Limpiar caché sin uso'),
      );
      expect(button.onPressed, isNull);
      expect(fake.cleaned, 1);
    },
  );
}
