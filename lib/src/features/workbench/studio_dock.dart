part of 'studio_panels.dart';

class StudioTransport extends ConsumerWidget {
  const StudioTransport({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) => LayoutBuilder(
    builder: (context, box) {
      final wide = box.maxWidth >= 1150;
      return Container(
        height: wide ? 184 : 222,
        padding: const EdgeInsets.fromLTRB(20, 16, 16, 16),
        decoration: const BoxDecoration(
          color: Color(0xFF0E141B),
          border: Border(top: BorderSide(color: Color(0xFF1A2531))),
        ),
        child:
            wide
                ? const Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Expanded(flex: 33, child: _DockPlayback()),
                    SizedBox(width: 24),
                    Expanded(flex: 34, child: _PitchDockPanel()),
                    SizedBox(width: 16),
                    Expanded(flex: 37, child: _DockMaster()),
                  ],
                )
                : const Column(
                  children: [
                    Expanded(
                      child: Row(
                        children: [
                          Expanded(child: _DockPlayback()),
                          SizedBox(width: 20),
                          Expanded(child: _DockMaster()),
                        ],
                      ),
                    ),
                    SizedBox(height: 12),
                    PitchControls(compact: true),
                  ],
                ),
      );
    },
  );
}

class _DockPlayback extends ConsumerWidget {
  const _DockPlayback();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final controller = ref.read(studioProvider.notifier);
    final enabled = state.session != null && !state.busy;
    return Row(
      children: [
        SizedBox(
          width: 68,
          height: 68,
          child: IconButton.filled(
            tooltip: state.playing ? 'Pausar' : 'Reproducir',
            style: IconButton.styleFrom(
              backgroundColor: AppColors.seaGlass,
              foregroundColor: const Color(0xFF04231F),
            ),
            onPressed:
                enabled
                    ? () =>
                        controller.transport(state.playing ? 'pause' : 'play')
                    : null,
            icon: Icon(
              state.playing ? Icons.pause : Icons.play_arrow,
              size: 32,
            ),
          ),
        ),
        const SizedBox(width: 12),
        IconButton(
          tooltip: 'Detener',
          onPressed: enabled ? () => controller.transport('stop') : null,
          icon: const Icon(Icons.stop_rounded, size: 20),
        ),
        IconButton(
          tooltip: 'Ir al inicio',
          onPressed: enabled ? () => controller.seek(0) : null,
          icon: const Icon(Icons.restart_alt, size: 21),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Container(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
            decoration: BoxDecoration(
              color: AppColors.canvas,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                FittedBox(
                  fit: BoxFit.scaleDown,
                  child: Text(
                    formatStudioTime(state.position),
                    style: const TextStyle(
                      fontSize: 30,
                      fontWeight: FontWeight.w700,
                      fontFeatures: [FontFeature.tabularFigures()],
                    ),
                  ),
                ),
                Text(
                  'de ${formatStudioTime(state.duration)}',
                  style: const TextStyle(fontSize: 9, color: AppColors.muted),
                ),
                const SizedBox(height: 6),
                Text(
                  state.playing ? 'REPRODUCIENDO' : 'EN PAUSA',
                  style: TextStyle(
                    fontSize: 7.2,
                    letterSpacing: 1,
                    color: state.playing ? AppColors.seaGlass : AppColors.muted,
                  ),
                ),
                SizedBox(
                  height: 15,
                  child: SliderTheme(
                    data: SliderTheme.of(context).copyWith(
                      overlayShape: SliderComponentShape.noOverlay,
                      thumbShape: const RoundSliderThumbShape(
                        enabledThumbRadius: 2,
                      ),
                    ),
                    child: Slider(
                      value: state.position.clamp(0, state.duration),
                      max: state.duration > 0 ? state.duration : 1,
                      semanticFormatterCallback:
                          (v) => 'Posición ${formatStudioTime(v)}',
                      onChanged: enabled ? controller.seek : null,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _PitchDockPanel extends StatelessWidget {
  const _PitchDockPanel();
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(
      color: AppColors.panel,
      borderRadius: BorderRadius.circular(12),
      border: Border.all(color: const Color(0xFF22302F)),
    ),
    child: const PitchControls(),
  );
}

class _DockMaster extends ConsumerWidget {
  const _DockMaster();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final enabled = state.session != null && !state.busy;
    return Row(
      children: [
        SizedBox(
          width: 70,
          child: Column(
            children: [
              const Text(
                'MASTER',
                style: TextStyle(
                  fontSize: 7.5,
                  letterSpacing: .8,
                  color: AppColors.muted,
                ),
              ),
              const SizedBox(height: 6),
              Expanded(
                child: Semantics(
                  label: 'Nivel de salida master',
                  value: '${state.peak}',
                  child: SizedBox(
                    width: 46,
                    child: CustomPaint(
                      painter: _MasterMeterPainter(state.peak),
                    ),
                  ),
                ),
              ),
              SizedBox(
                height: 20,
                child: SliderTheme(
                  data: SliderTheme.of(
                    context,
                  ).copyWith(overlayShape: SliderComponentShape.noOverlay),
                  child: Slider(
                    value: state.master.clamp(0, 2),
                    max: 2,
                    semanticFormatterCallback:
                        (v) => 'Ganancia master ${v.toStringAsFixed(2)}',
                    onChanged:
                        enabled
                            ? ref.read(studioProvider.notifier).setMaster
                            : null,
                  ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(width: 16),
        Expanded(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 8,
                      ),
                      decoration: BoxDecoration(
                        color: AppColors.seaGlass.withValues(alpha: .08),
                        borderRadius: BorderRadius.circular(20),
                        border: Border.all(
                          color: AppColors.seaGlass.withValues(alpha: .18),
                        ),
                      ),
                      child: Text(
                        state.busy ? 'Procesando…' : '✓ Separación lista',
                        style: const TextStyle(
                          fontSize: 10,
                          color: AppColors.seaGlass,
                        ),
                      ),
                    ),
                  ),
                  IconButton(
                    tooltip: 'Abrir carpeta de la sesión',
                    onPressed:
                        enabled
                            ? () => openStudioFolder(
                              context,
                              state.session!.directory,
                            )
                            : null,
                    icon: const Icon(Icons.folder_open_outlined, size: 18),
                  ),
                ],
              ),
              const SizedBox(height: 10),
              SizedBox(
                height: 56,
                child: FilledButton.icon(
                  onPressed:
                      enabled
                          ? () => showDialog<void>(
                            context: context,
                            builder: (_) => const ExportDialog(),
                          )
                          : null,
                  icon: const Icon(Icons.file_download_outlined, size: 20),
                  label: const Text(
                    'Exportar stems',
                    style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
                  ),
                  style: FilledButton.styleFrom(
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(12),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _MasterMeterPainter extends CustomPainter {
  _MasterMeterPainter(this.peak);
  final double peak;
  @override
  void paint(Canvas canvas, Size size) {
    final normalized = peak.isFinite ? peak.clamp(0.0, 1.0) : 0.0;
    for (var i = 0; i < 22; i++) {
      final y = size.height - (i + 1) * size.height / 22;
      final color =
          i > 19
              ? AppColors.coral
              : i > 16
              ? AppColors.amber
              : const Color(0xFF22C55E);
      final paint =
          Paint()
            ..color =
                i / 22 < normalized ? color : color.withValues(alpha: .10);
      canvas.drawRect(Rect.fromLTWH(12, y, 9, size.height / 22 - 2), paint);
      canvas.drawRect(Rect.fromLTWH(24, y, 9, size.height / 22 - 2), paint);
    }
  }

  @override
  bool shouldRepaint(_MasterMeterPainter old) => old.peak != peak;
}

class PitchControls extends ConsumerWidget {
  const PitchControls({super.key, this.compact = false});
  final bool compact;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final semitones = state.requestedPitch;
    final formants = state.requestedFormants;
    final enabled =
        state.session != null && state.livePitch != null && !state.busy;
    final pitch = Container(
      decoration: BoxDecoration(
        color: AppColors.canvas,
        borderRadius: BorderRadius.circular(6),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          IconButton(
            tooltip: 'Bajar tono',
            onPressed:
                enabled && semitones > -12
                    ? () => ref
                        .read(studioProvider.notifier)
                        .applyPitch(semitones - 1, formants)
                    : null,
            icon: const Icon(Icons.remove, size: 15),
          ),
          Text(
            '${semitones > 0 ? "+" : ""}$semitones st',
            style: const TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              fontFeatures: [FontFeature.tabularFigures()],
            ),
          ),
          IconButton(
            tooltip: 'Subir tono',
            onPressed:
                enabled && semitones < 12
                    ? () => ref
                        .read(studioProvider.notifier)
                        .applyPitch(semitones + 1, formants)
                    : null,
            icon: const Icon(Icons.add, size: 15),
          ),
        ],
      ),
    );
    final formant = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        SizedBox(
          height: 28,
          child: Transform.scale(
            scale: .65,
            child: Switch(
              value: formants,
              onChanged:
                  enabled
                      ? (v) => ref
                          .read(studioProvider.notifier)
                          .applyPitch(semitones, v)
                      : null,
            ),
          ),
        ),
        Text(
          formants ? 'Activo' : 'Inactivo',
          style: const TextStyle(fontSize: 9, color: AppColors.seaGlass),
        ),
        IconButton(
          tooltip: 'Restablecer tono',
          onPressed:
              enabled
                  ? () =>
                      ref.read(studioProvider.notifier).applyPitch(0, formants)
                  : null,
          icon: const Icon(Icons.restart_alt, size: 15),
        ),
      ],
    );
    if (compact) {
      return Wrap(
        spacing: 14,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          const Text(
            'PITCHER',
            style: TextStyle(fontSize: 9, color: AppColors.muted),
          ),
          pitch,
          const Text(
            'FORMANTE',
            style: TextStyle(fontSize: 8, color: AppColors.muted),
          ),
          formant,
        ],
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Row(
          children: [
            Icon(Icons.tune, size: 14, color: AppColors.seaGlass),
            SizedBox(width: 8),
            Text(
              'PITCHER',
              style: TextStyle(fontSize: 9.4, fontWeight: FontWeight.w600),
            ),
            Spacer(),
            Text(
              'En vivo',
              style: TextStyle(fontSize: 8, color: AppColors.seaGlass),
            ),
          ],
        ),
        const SizedBox(height: 5),
        const Text(
          'Corrección de tono · todas las pistas',
          style: TextStyle(fontSize: 8.6, color: AppColors.muted),
        ),
        const Spacer(),
        Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'PITCH',
                    style: TextStyle(fontSize: 7.2, color: AppColors.muted),
                  ),
                  const SizedBox(height: 5),
                  pitch,
                ],
              ),
            ),
            const SizedBox(width: 10),
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'FORMANTE',
                  style: TextStyle(fontSize: 7.2, color: AppColors.muted),
                ),
                const SizedBox(height: 5),
                formant,
              ],
            ),
          ],
        ),
        const SizedBox(height: 3),
        SizedBox(
          height: 20,
          child: Row(
            children: [
              const Text(
                '−12 st',
                style: TextStyle(fontSize: 8, color: AppColors.muted),
              ),
              Expanded(
                child: Slider(
                  value: semitones.toDouble(),
                  min: -12,
                  max: 12,
                  divisions: 24,
                  semanticFormatterCallback:
                      (v) => 'Tono ${v.round()} semitonos',
                  onChanged:
                      enabled
                          ? (v) => ref
                              .read(studioProvider.notifier)
                              .applyPitch(v.round(), formants)
                          : null,
                ),
              ),
              const Text(
                '+12 st',
                style: TextStyle(fontSize: 8, color: AppColors.muted),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class PitchDialog extends StatelessWidget {
  const PitchDialog({super.key});
  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Tono en vivo'),
    content: const SizedBox(width: 570, height: 150, child: PitchControls()),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cerrar'),
      ),
    ],
  );
}
