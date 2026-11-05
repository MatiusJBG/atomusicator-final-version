import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/di/compute_settings.dart';
import '../../../core/di/studio_controller.dart';

class ComputeSettingsPanel extends ConsumerWidget {
  const ComputeSettingsPanel({super.key, this.compact = false});
  final bool compact;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(computeSettingsProvider);
    final busy = ref.watch(studioProvider.select((s) => s.busy));
    return settings.when(
      loading: () => const LinearProgressIndicator(),
      error:
          (error, _) => Row(
            children: [
              const Expanded(
                child: Text('No se pudo consultar el perfil del motor.'),
              ),
              TextButton(
                onPressed: () => ref.invalidate(computeSettingsProvider),
                child: const Text('Reintentar'),
              ),
            ],
          ),
      data:
          (value) => Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('PERFIL DE CÓMPUTO'),
              Wrap(
                spacing: 8,
                children: [
                  for (final profile in value.availableProfiles)
                    ChoiceChip(
                      label: Text(switch (profile) {
                        'eco' => 'Ahorro',
                        'balanced' => 'Equilibrado',
                        'throughput' => 'Máxima velocidad',
                        _ => profile,
                      }),
                      selected: value.profile == profile,
                      onSelected:
                          busy
                              ? null
                              : (_) => ref
                                  .read(computeSettingsProvider.notifier)
                                  .select(profile),
                    ),
                ],
              ),
              if (!compact)
                Text(
                  'Hilos solicitados: ${value.requestedThreads} · efectivos: ${value.effectiveThreads}',
                ),
              if (!compact)
                const Text(
                  'Se guarda en el motor para los próximos trabajos. La reproducción puede reducir los hilos efectivos.',
                ),
            ],
          ),
    );
  }
}
