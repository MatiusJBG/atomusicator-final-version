import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/di/studio_controller.dart';
import '../../rust_bridge/api.dart' as native;
import 'particle_loading_widgets.dart';

abstract class CacheSettingsGateway {
  Future<native.UiCacheSettingsDto> read();
  Future<void> save(BigInt bytes);
  Future<BigInt> clean();
}

class NativeCacheSettingsGateway implements CacheSettingsGateway {
  @override
  Future<native.UiCacheSettingsDto> read() => native.cacheSettings();
  @override
  Future<void> save(BigInt bytes) => native.setCacheRetainedBytes(bytes: bytes);
  @override
  Future<BigInt> clean() => native.cleanRetainedCache();
}

final cacheGatewayProvider = Provider<CacheSettingsGateway>(
  (ref) => NativeCacheSettingsGateway(),
);
final cacheSettingsProvider = FutureProvider.autoDispose((ref) async {
  await ref.watch(nativeEngineStatusProvider.future);
  return ref.watch(cacheGatewayProvider).read();
});

class CacheSettingsPanel extends ConsumerStatefulWidget {
  const CacheSettingsPanel({super.key});
  @override
  ConsumerState<CacheSettingsPanel> createState() => _CacheSettingsPanelState();
}

class _CacheSettingsPanelState extends ConsumerState<CacheSettingsPanel> {
  bool working = false;
  String? message;
  Future<void> change(BigInt? bytes) async {
    if (working || ref.read(studioProvider).busy) return;
    setState(() {
      working = true;
      message = null;
    });
    final gateway = ref.read(cacheGatewayProvider);
    try {
      if (bytes == null) {
        final removed = await gateway.clean();
        if (mounted) {
          message =
              'Liberados ${(removed.toDouble() / 1073741824).toStringAsFixed(2)} GiB.';
        }
      } else {
        await gateway.save(bytes);
      }
      if (mounted) ref.invalidate(cacheSettingsProvider);
    } catch (_) {
      if (mounted) {
        message =
            'El motor no pudo completar la operación. Comprueba el espacio y los permisos.';
      }
    } finally {
      if (mounted) setState(() => working = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final busy = ref.watch(studioProvider.select((s) => s.busy));
    return ref
        .watch(cacheSettingsProvider)
        .when(
          loading: () => const ParticleProgressBar(),
          error:
              (_, _) => TextButton(
                onPressed: () => ref.invalidate(cacheSettingsProvider),
                child: const Text('Reintentar consulta de caché'),
              ),
          data: (value) {
            final options =
                {
                    value.retainedBytes,
                    for (final n in [0, 4, 8, 16, 32, 64, 128])
                      BigInt.from(n) * BigInt.from(1073741824),
                  }.toList()
                  ..sort();
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('ALMACENAMIENTO TEMPORAL · CACHÉ RETENIDA'),
                Text(
                  'Ocupado: ${(value.usedBytes.toDouble() / 1073741824).toStringAsFixed(2)} GiB',
                ),
                DropdownButton<BigInt>(
                  isExpanded: true,
                  value: value.retainedBytes,
                  items: [
                    for (final bytes in options)
                      DropdownMenuItem(
                        value: bytes,
                        child: Text(
                          'Conservar ${(bytes.toDouble() / 1073741824).toStringAsFixed(1)} GiB',
                        ),
                      ),
                  ],
                  onChanged:
                      busy || working
                          ? null
                          : (bytes) {
                            if (bytes != null) change(bytes);
                          },
                ),
                const Text(
                  'Los archivos de trabajos activos pueden superar esta cuota. El motor verifica el espacio libre por separado. Una variable de entorno puede prevalecer sobre el valor guardado.',
                ),
                Text(
                  value.directory,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                TextButton(
                  onPressed: busy || working ? null : () => change(null),
                  child: const Text('Limpiar caché sin uso'),
                ),
                if (message != null) Text(message!),
              ],
            );
          },
        );
  }
}
