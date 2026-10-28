part of 'studio_panels.dart';

class StudioTransport extends ConsumerWidget {
  const StudioTransport({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final controller = ref.read(studioProvider.notifier);
    final enabled = state.session != null && !state.busy;
    return Container(
      decoration: const BoxDecoration(
        color: AppColors.panel,
        border: Border(top: BorderSide(color: AppColors.line)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
      child: LayoutBuilder(
        builder: (context, box) {
          final wide = box.maxWidth >= 1050;
          final playback = SizedBox(
            width: 320,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  state.playing ? '●  Playing' : '●  Playback',
                  style: TextStyle(
                    fontSize: 10,
                    color: state.playing ? AppColors.seaGlass : AppColors.muted,
                  ),
                ),
                const SizedBox(height: 10),
                Row(
                  children: [
                    IconButton.filled(
                      tooltip: state.playing ? 'Pause' : 'Play',
                      style: IconButton.styleFrom(
                        backgroundColor: AppColors.seaGlass,
                        foregroundColor: AppColors.canvas,
                        fixedSize: const Size(46, 46),
                      ),
                      onPressed:
                          enabled
                              ? () => controller.transport(
                                state.playing ? 'pause' : 'play',
                              )
                              : null,
                      icon: Icon(
                        state.playing ? Icons.pause : Icons.play_arrow,
                      ),
                    ),
                    IconButton(
                      tooltip: 'Stop',
                      onPressed:
                          enabled ? () => controller.transport('stop') : null,
                      icon: const Icon(Icons.stop_rounded, size: 19),
                    ),
                    IconButton(
                      tooltip: 'Go to start',
                      onPressed: enabled ? () => controller.seek(0) : null,
                      icon: const Icon(Icons.first_page, size: 19),
                    ),
                    Expanded(
                      child: FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: Alignment.centerLeft,
                        child: Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 10,
                            vertical: 9,
                          ),
                          decoration: BoxDecoration(
                            color: AppColors.canvas,
                            border: Border.all(color: AppColors.line),
                            borderRadius: BorderRadius.circular(5),
                          ),
                          child: Text(
                            '${formatStudioTime(state.position)} / ${formatStudioTime(state.duration)}',
                            style: const TextStyle(
                              fontSize: 13,
                              fontFeatures: [FontFeature.tabularFigures()],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
                SizedBox(
                  height: 22,
                  width: 295,
                  child: Slider(
                    value: state.position.clamp(0, state.duration),
                    max: state.duration > 0 ? state.duration : 1,
                    semanticFormatterCallback:
                        (v) => 'Position ${formatStudioTime(v)}',
                    onChanged: enabled ? controller.seek : null,
                  ),
                ),
              ],
            ),
          );
          final master = SizedBox(
            width: 330,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    const Text(
                      'Master',
                      style: TextStyle(fontSize: 10, color: AppColors.muted),
                    ),
                    const Spacer(),
                    if (state.session != null)
                      Text(
                        state.busy ? 'Processing…' : '✓  Stems ready',
                        style: const TextStyle(
                          fontSize: 10,
                          color: AppColors.seaGlass,
                        ),
                      ),
                  ],
                ),
                const SizedBox(height: 10),
                Row(
                  children: [
                    SizedBox(
                      width: 100,
                      child: Column(
                        children: [
                          LinearProgressIndicator(
                            value: state.peak.clamp(0, 1),
                            minHeight: 7,
                            semanticsLabel: 'Master audio level',
                            color:
                                state.peak > 1
                                    ? AppColors.coral
                                    : AppColors.seaGlass,
                            backgroundColor: AppColors.line,
                          ),
                          SizedBox(
                            height: 27,
                            child: Slider(
                              value: state.master,
                              max: 2,
                              semanticFormatterCallback:
                                  (v) => 'Master gain ${v.toStringAsFixed(2)}',
                              onChanged: enabled ? controller.setMaster : null,
                            ),
                          ),
                        ],
                      ),
                    ),
                    const Spacer(),
                    Tooltip(
                      message: 'Export stems',
                      child: FilledButton.icon(
                        onPressed:
                            enabled
                                ? () => showDialog<void>(
                                  context: context,
                                  builder: (_) => const ExportDialog(),
                                )
                                : null,
                        icon: const Icon(
                          Icons.file_download_outlined,
                          size: 17,
                        ),
                        label: const Text('Export stems'),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          );
          if (wide) {
            return Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                playback,
                const SizedBox(height: 88, child: VerticalDivider(width: 25)),
                const Expanded(child: PitchControls()),
                const SizedBox(height: 88, child: VerticalDivider(width: 25)),
                master,
              ],
            );
          }
          return Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(children: [playback, const Spacer(), master]),
              const Divider(height: 10),
              const PitchControls(compact: true),
            ],
          );
        },
      ),
    );
  }
}

class PitchControls extends ConsumerWidget {
  const PitchControls({super.key, this.compact = false});
  final bool compact;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final semitones = state.requestedPitch;
    final formants = state.requestedFormants;
    final enabled = state.session != null && state.livePitch != null;
    final controls = Row(
      children: [
        Container(
          decoration: BoxDecoration(
            color: AppColors.canvas,
            border: Border.all(color: AppColors.line),
            borderRadius: BorderRadius.circular(5),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                tooltip: 'Lower pitch',
                visualDensity: VisualDensity.compact,
                onPressed:
                    enabled && semitones > -12
                        ? () => ref
                            .read(studioProvider.notifier)
                            .applyPitch(semitones - 1, formants)
                        : null,
                icon: const Icon(Icons.remove, size: 14),
              ),
              Text(
                'Pitch: ${semitones > 0 ? '+' : ''}$semitones st',
                style: const TextStyle(fontSize: 11),
              ),
              IconButton(
                tooltip: 'Raise pitch',
                visualDensity: VisualDensity.compact,
                onPressed:
                    enabled && semitones < 12
                        ? () => ref
                            .read(studioProvider.notifier)
                            .applyPitch(semitones + 1, formants)
                        : null,
                icon: const Icon(Icons.add, size: 14),
              ),
            ],
          ),
        ),
        const Spacer(),
        SizedBox(
          height: 28,
          child: Transform.scale(
            scale: .7,
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
        const Text(
          'Formants',
          style: TextStyle(fontSize: 10, color: AppColors.muted),
        ),
        const Spacer(),
        const Text(
          'Live',
          style: TextStyle(fontSize: 10, color: AppColors.muted),
        ),
        IconButton(
          tooltip: 'Reset pitch',
          onPressed:
              enabled
                  ? () =>
                      ref.read(studioProvider.notifier).applyPitch(0, formants)
                  : null,
          icon: const Icon(Icons.restart_alt, size: 16),
        ),
      ],
    );
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (!compact) ...[
          const Row(
            children: [
              Icon(Icons.tune, size: 16),
              SizedBox(width: 6),
              Text(
                'Pitch',
                style: TextStyle(fontSize: 11, color: AppColors.muted),
              ),
              Spacer(),
              Text(
                'All tracks · Original tempo',
                style: TextStyle(fontSize: 9, color: AppColors.muted),
              ),
            ],
          ),
          const SizedBox(height: 10),
        ],
        controls,
        if (!compact)
          SizedBox(
            height: 26,
            child: Row(
              children: [
                const Text(
                  '−12',
                  style: TextStyle(fontSize: 9, color: AppColors.muted),
                ),
                Expanded(
                  child: Slider(
                    value: semitones.toDouble(),
                    min: -12,
                    max: 12,
                    divisions: 24,
                    semanticFormatterCallback:
                        (v) => 'Pitch ${v.round()} semitones',
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
                  style: TextStyle(fontSize: 9, color: AppColors.muted),
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
    title: const Text('Live pitch'),
    content: const SizedBox(width: 570, child: PitchControls()),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Close'),
      ),
    ],
  );
}
