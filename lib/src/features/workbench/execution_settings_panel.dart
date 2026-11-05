import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/di/studio_controller.dart';
import '../../rust_bridge/api.dart' as native;

abstract class ExecutionSettingsGateway {
  Future<native.UiExecutionSettingsDto> read();
  Future<void> save(String mode);
}

class NativeExecutionSettingsGateway implements ExecutionSettingsGateway {
  @override
  Future<native.UiExecutionSettingsDto> read() => native.executionSettings();
  @override
  Future<void> save(String mode) => native.setExecutionMode(mode: mode);
}

final executionGatewayProvider = Provider<ExecutionSettingsGateway>(
  (ref) => NativeExecutionSettingsGateway(),
);
final executionSettingsProvider = FutureProvider.autoDispose((ref) async {
  await ref.watch(nativeEngineStatusProvider.future);
  return ref.watch(executionGatewayProvider).read();
});

class ExecutionSettingsPanel extends ConsumerStatefulWidget {
  const ExecutionSettingsPanel({super.key});
  @override
  ConsumerState<ExecutionSettingsPanel> createState() =>
      _ExecutionSettingsPanelState();
}

class _ExecutionSettingsPanelState
    extends ConsumerState<ExecutionSettingsPanel> {
  bool saving = false;
  String? error;
  Future<void> save(String mode) async {
    if (saving || ref.read(studioProvider).busy) return;
    setState(() {
      saving = true;
      error = null;
    });
    final gateway = ref.read(executionGatewayProvider);
    try {
      await gateway.save(mode);
      if (mounted) ref.invalidate(executionSettingsProvider);
    } catch (_) {
      if (mounted) {
        error =
            'No se pudo guardar el proveedor. Reintenta cuando el motor esté libre.';
      }
    } finally {
      if (mounted) setState(() => saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final settings = ref.watch(executionSettingsProvider);
    final busy = ref.watch(studioProvider.select((s) => s.busy));
    return settings.when(
      loading: () => const LinearProgressIndicator(),
      error:
          (_, _) => TextButton(
            onPressed: () => ref.invalidate(executionSettingsProvider),
            child: const Text('Reintentar consulta de aceleración'),
          ),
      data: (value) {
        List<dynamic> diagnostics;
        try {
          diagnostics = jsonDecode(value.diagnosticsJson) as List;
        } catch (_) {
          diagnostics = [];
        }
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DropdownButton<String>(
              value: value.nextStartupMode,
              isExpanded: true,
              items: const [
                DropdownMenuItem(
                  value: 'cpu',
                  child: Text('CPU · máxima compatibilidad'),
                ),
                DropdownMenuItem(
                  value: 'directml',
                  child: Text('DirectML · GPU Windows'),
                ),
                DropdownMenuItem(
                  value: 'auto',
                  child: Text('Automático · GPU con respaldo CPU'),
                ),
              ],
              onChanged:
                  saving || busy
                      ? null
                      : (mode) {
                        if (mode != null) save(mode);
                      },
            ),
            const Text(
              'Se aplica al reiniciar Atomusicator. Una variable de entorno puede prevalecer sobre esta preferencia.',
            ),
            if (error != null) Text(error!),
            if (diagnostics.isEmpty)
              const Text('Sin diagnósticos de inferencia en esta ejecución.'),
            for (final entry in diagnostics.whereType<Map>()) ...[
              Text(
                'Registrado: ${entry['provider_registered']} · activo: ${entry['provider_effective'] ?? 'asignación sin confirmar'}',
              ),
              if ((entry['fallback_errors'] as List?)?.isNotEmpty ?? false)
                Text(
                  'Respaldo del motor: ${(entry['fallback_errors'] as List).join('; ')}',
                ),
            ],
            TextButton(
              onPressed: () => ref.invalidate(executionSettingsProvider),
              child: const Text('Actualizar diagnóstico'),
            ),
          ],
        );
      },
    );
  }
}
