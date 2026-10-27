import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/di/studio_controller.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/di/workbench_providers.dart';
import '../../../core/theme/app_theme.dart';
import '../../rust_bridge/api.dart' as native;

const stemNames = {
  'lead_vocal': 'Lead vocal',
  'backing_vocals': 'Backing vocals',
  'drums': 'Drums',
  'bass': 'Bass',
  'instrumental': 'Instrumental',
};
final timelineZoomProvider = StateProvider<double>((ref) => 1);

class TrackLevelMeter extends StatelessWidget {
  const TrackLevelMeter({
    super.key,
    required this.stemType,
    required this.peak,
  });
  final String stemType;
  final double peak;

  @override
  Widget build(BuildContext context) {
    final level = peak.isFinite ? peak.clamp(0.0, 1.0) : 0.0;
    final name = stemNames[stemType] ?? stemType;
    return Tooltip(
      message:
          '$name level: ${peak > 1 ? 'clipping' : 'post fader, before master'}',
      child: LinearProgressIndicator(
        key: ValueKey('meter-$stemType'),
        value: level,
        minHeight: 4,
        color: peak > 1 ? AppColors.coral : AppColors.seaGlass,
        backgroundColor: AppColors.line,
        semanticsLabel: '$name audio level',
      ),
    );
  }
}

class StudioActions extends ConsumerWidget {
  const StudioActions({super.key});
  Future<void> _open(BuildContext context, WidgetRef ref) async {
    final directory = await FilePicker.platform.getDirectoryPath(
      dialogTitle: 'Open a session folder containing session.json',
    );
    if (directory != null) {
      await ref.read(studioProvider.notifier).open(directory);
    }
  }

  Future<void> _projects(BuildContext context, WidgetRef ref) async {
    try {
      await ref.read(nativeEngineStatusProvider.future);
      final current = await native.projectsDirectory();
      final directory = await FilePicker.platform.getDirectoryPath(
        dialogTitle: 'Projects directory',
        initialDirectory: current,
      );
      if (directory != null) {
        await native.setProjectsDirectory(directory: directory);
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('$error')));
      }
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final available = ref.watch(nativeEngineStatusProvider).hasValue;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 5),
      child: Wrap(
        spacing: 8,
        runSpacing: 4,
        children: [
          TextButton.icon(
            onPressed:
                available && !state.busy ? () => _open(context, ref) : null,
            icon: const Icon(Icons.folder_open, size: 18),
            label: const Text('Open session'),
          ),
          TextButton.icon(
            onPressed:
                available && !state.busy ? () => _projects(context, ref) : null,
            icon: const Icon(Icons.settings_outlined, size: 18),
            label: const Text('Projects folder'),
          ),
          TextButton.icon(
            onPressed:
                state.session != null && !state.busy
                    ? () => showDialog<void>(
                      context: context,
                      builder: (_) => const PitchDialog(),
                    )
                    : null,
            icon: const Icon(Icons.tune, size: 18),
            label: Text('Pitch ${state.session?.pitchSemitones ?? 0} st'),
          ),
          TextButton.icon(
            onPressed:
                state.session != null && !state.busy
                    ? () => showDialog<void>(
                      context: context,
                      builder: (_) => const ExportDialog(),
                    )
                    : null,
            icon: const Icon(Icons.file_upload_outlined, size: 18),
            label: const Text('Export stems'),
          ),
        ],
      ),
    );
  }
}

class StudioFeedback extends ConsumerWidget {
  const StudioFeedback({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final job = state.job;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 140),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (state.busy) ...[
              LinearProgressIndicator(
                value: job?.status == 'running' ? job!.progress : null,
              ),
              Row(
                children: [
                  Expanded(
                    child: Text(job?.message ?? 'Starting…', maxLines: 2),
                  ),
                  TextButton(
                    onPressed:
                        job?.status == 'running'
                            ? () => ref.read(studioProvider.notifier).cancel()
                            : null,
                    child: const Text('Cancel'),
                  ),
                ],
              ),
            ],
            if (state.error != null)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 5),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SelectableText(
                      state.error!,
                      style: const TextStyle(color: AppColors.coral),
                    ),
                    if (state.error!.contains('Authorize'))
                      TextButton(
                        onPressed: () async {
                          final sources = ref.read(workbenchProvider).sources;
                          if (sources.isEmpty) return;
                          final approved = await showDialog<bool>(
                            context: context,
                            builder:
                                (context) => AlertDialog(
                                  title: const Text(
                                    'Download separation models?',
                                  ),
                                  content: const Text(
                                    'Download the four reviewed UVR/Kuielab models (about 180 MB). SHA-256 is checked before inference. Models are cached locally; your audio stays on this computer. UVR, Anjok07, aufr33 and Kuielab/KINoAI — MIT.',
                                  ),
                                  actions: [
                                    TextButton(
                                      onPressed:
                                          () => Navigator.pop(context, false),
                                      child: const Text('Cancel'),
                                    ),
                                    FilledButton(
                                      onPressed:
                                          () => Navigator.pop(context, true),
                                      child: const Text(
                                        'Download and separate',
                                      ),
                                    ),
                                  ],
                                ),
                          );
                          if (approved == true) {
                            await ref
                                .read(studioProvider.notifier)
                                .separate(
                                  ref
                                      .read(workbenchProvider)
                                      .selectedSource!
                                      .path,
                                  allowDownload: true,
                                );
                          }
                        },
                        child: const Text('Download models and retry'),
                      ),
                  ],
                ),
              ),
            if (job?.status == 'cancelled')
              const Text('Operation cancelled. Previous session retained.'),
            if (job?.status == 'completed' && job?.outputDirectory != null)
              SelectableText('Exported to ${job!.outputDirectory}'),
          ],
        ),
      ),
    );
  }
}

class StudioTracks extends ConsumerWidget {
  const StudioTracks({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final controller = ref.read(studioProvider.notifier);
    final zoom = ref.watch(timelineZoomProvider);
    final tracks = ListView.separated(
      itemCount: state.session!.tracks.length,
      padding: const EdgeInsets.all(8),
      separatorBuilder: (_, __) => const SizedBox(height: 8),
      itemBuilder: (context, index) {
        final track = state.session!.tracks[index];
        final mix = state.tracks[track.stemType] ?? const MixTrackState();
        final enabled = !state.busy;
        final colors = [
          AppColors.seaGlass,
          AppColors.lavender,
          AppColors.amber,
          AppColors.blue,
          AppColors.coral,
        ];
        return Container(
          padding: const EdgeInsets.all(10),
          decoration: BoxDecoration(
            color: AppColors.panel,
            borderRadius: BorderRadius.circular(10),
            border: Border.all(color: AppColors.line),
          ),
          child: Column(
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      stemNames[track.stemType] ?? track.stemType,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                  ),
                  FilterChip(
                    label: const Text('M'),
                    selected: mix.mute,
                    onSelected:
                        enabled
                            ? (value) => controller.setTrack(
                              track.stemType,
                              mix.copyWith(mute: value),
                            )
                            : null,
                  ),
                  const SizedBox(width: 5),
                  FilterChip(
                    label: const Text('S'),
                    selected: mix.solo,
                    onSelected:
                        enabled
                            ? (value) => controller.setTrack(
                              track.stemType,
                              mix.copyWith(solo: value),
                            )
                            : null,
                  ),
                ],
              ),
              TrackLevelMeter(
                stemType: track.stemType,
                peak: state.trackPeaks[track.stemType] ?? 0,
              ),
              const SizedBox(height: 4),
              LayoutBuilder(
                builder:
                    (context, box) => GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTapDown:
                          enabled
                              ? (details) => controller.seek(
                                (details.localPosition.dx / box.maxWidth).clamp(
                                      0,
                                      1,
                                    ) *
                                    state.duration,
                              )
                              : null,
                      child: SizedBox(
                        height: 52,
                        width: double.infinity,
                        child: CustomPaint(
                          painter: WaveformPainter(
                            peaks: track.peaks,
                            color: colors[index % colors.length],
                            progress:
                                state.duration > 0
                                    ? state.position / state.duration
                                    : 0,
                          ),
                        ),
                      ),
                    ),
              ),
              Row(
                children: [
                  const Icon(Icons.volume_up_outlined, size: 16),
                  Expanded(
                    child: Slider(
                      value: mix.gainDb.clamp(-60, 12),
                      min: -60,
                      max: 12,
                      label: '${mix.gainDb.toStringAsFixed(1)} dB',
                      onChanged:
                          enabled
                              ? (value) => controller.setTrack(
                                track.stemType,
                                mix.copyWith(gainDb: value),
                              )
                              : null,
                    ),
                  ),
                  SizedBox(
                    width: 58,
                    child: Text('${mix.gainDb.toStringAsFixed(1)} dB'),
                  ),
                  const Text('Pan'),
                  Expanded(
                    child: Slider(
                      value: mix.pan,
                      min: -1,
                      max: 1,
                      onChanged:
                          enabled
                              ? (value) => controller.setTrack(
                                track.stemType,
                                mix.copyWith(pan: value),
                              )
                              : null,
                    ),
                  ),
                ],
              ),
            ],
          ),
        );
      },
    );
    return Column(
      children: [
        Row(
          children: [
            const Text('Timeline'),
            const Spacer(),
            DropdownButton<double>(
              value: zoom,
              items:
                  [1.0, 2.0, 3.0, 4.0]
                      .map(
                        (value) => DropdownMenuItem(
                          value: value,
                          child: Text('${(value * 100).round()}%'),
                        ),
                      )
                      .toList(),
              onChanged: (value) {
                if (value != null) {
                  ref.read(timelineZoomProvider.notifier).state = value;
                }
              },
            ),
          ],
        ),
        Expanded(
          child: LayoutBuilder(
            builder:
                (context, box) => Scrollbar(
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: SizedBox(
                      width: box.maxWidth * zoom,
                      height: box.maxHeight,
                      child: Column(
                        children: [
                          SizedBox(
                            height: 24,
                            child: CustomPaint(
                              size: Size(box.maxWidth * zoom, 24),
                              painter: TimelineRulerPainter(state.duration),
                            ),
                          ),
                          Expanded(child: tracks),
                        ],
                      ),
                    ),
                  ),
                ),
          ),
        ),
      ],
    );
  }
}

class TimelineRulerPainter extends CustomPainter {
  TimelineRulerPainter(this.duration);
  final double duration;
  @override
  void paint(Canvas canvas, Size size) {
    final text = TextPainter(textDirection: TextDirection.ltr);
    for (var i = 0; i <= 8; i++) {
      final seconds = duration * i / 8;
      text.text = TextSpan(
        text: '${seconds.toStringAsFixed(1)}s',
        style: const TextStyle(fontSize: 10, color: AppColors.muted),
      );
      text.layout();
      text.paint(
        canvas,
        Offset(
          (size.width * i / 8).clamp(
            0,
            (size.width - text.width).clamp(0, double.infinity),
          ),
          2,
        ),
      );
    }
  }

  @override
  bool shouldRepaint(TimelineRulerPainter old) => old.duration != duration;
}

class WaveformPainter extends CustomPainter {
  WaveformPainter({
    required this.peaks,
    required this.color,
    required this.progress,
  });
  final List<double> peaks;
  final Color color;
  final double progress;
  @override
  void paint(Canvas canvas, Size size) {
    final pen =
        Paint()
          ..color = color
          ..strokeWidth = 1;
    final count = peaks.length ~/ 2;
    for (var i = 0; i < count; i++) {
      final x = i * size.width / count;
      canvas.drawLine(
        Offset(x, size.height * (0.5 - peaks[i * 2].clamp(-1, 1) * 0.45)),
        Offset(x, size.height * (0.5 - peaks[i * 2 + 1].clamp(-1, 1) * 0.45)),
        pen,
      );
    }
    pen
      ..color = Colors.white70
      ..strokeWidth = 1.5;
    final x = progress.clamp(0, 1) * size.width;
    canvas.drawLine(Offset(x, 0), Offset(x, size.height), pen);
  }

  @override
  bool shouldRepaint(WaveformPainter old) =>
      old.peaks != peaks || old.progress != progress || old.color != color;
}

class StudioTransport extends ConsumerWidget {
  const StudioTransport({super.key});
  String _time(double value) {
    final seconds = value.floor();
    return '${seconds ~/ 60}:${(seconds % 60).toString().padLeft(2, '0')}';
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final controller = ref.read(studioProvider.notifier);
    final enabled = state.session != null && !state.busy;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      color: AppColors.panel,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              IconButton(
                tooltip: 'Go to start',
                onPressed: enabled ? () => controller.seek(0) : null,
                icon: const Icon(Icons.skip_previous_rounded),
              ),
              IconButton(
                tooltip: state.playing ? 'Pause' : 'Play',
                onPressed:
                    enabled
                        ? () => controller.transport(
                          state.playing ? 'pause' : 'play',
                        )
                        : null,
                icon: Icon(
                  state.playing
                      ? Icons.pause_rounded
                      : Icons.play_arrow_rounded,
                  color: AppColors.seaGlass,
                ),
              ),
              IconButton(
                tooltip: 'Stop',
                onPressed: enabled ? () => controller.transport('stop') : null,
                icon: const Icon(Icons.stop_rounded),
              ),
              Text('${_time(state.position)} / ${_time(state.duration)}'),
              const Spacer(),
              const Icon(Icons.volume_up_outlined, size: 18),
              SizedBox(
                width: 100,
                child: Slider(
                  value: state.master,
                  max: 2,
                  onChanged: enabled ? controller.setMaster : null,
                ),
              ),
              SizedBox(
                width: 42,
                child: LinearProgressIndicator(
                  value: state.peak.clamp(0, 1),
                  color: state.peak > 1 ? AppColors.coral : AppColors.seaGlass,
                ),
              ),
            ],
          ),
          SizedBox(
            height: 24,
            child: Slider(
              value: state.position.clamp(0, state.duration),
              max: state.duration > 0 ? state.duration : 1,
              onChanged: enabled ? controller.seek : null,
            ),
          ),
        ],
      ),
    );
  }
}

class PitchDialog extends ConsumerStatefulWidget {
  const PitchDialog({super.key});
  @override
  ConsumerState<PitchDialog> createState() => _PitchDialogState();
}

class _PitchDialogState extends ConsumerState<PitchDialog> {
  double semitones = 0;
  bool formants = false;
  @override
  void initState() {
    super.initState();
    semitones =
        ref.read(studioProvider).session?.pitchSemitones.toDouble() ?? 0;
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Change pitch'),
    content: SizedBox(
      width: 380,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('${semitones.round()} semitones · original tempo'),
          Slider(
            value: semitones,
            min: -12,
            max: 12,
            divisions: 24,
            onChanged: (value) => setState(() => semitones = value),
          ),
          CheckboxListTile(
            title: const Text('Preserve formants'),
            value: formants,
            onChanged: (value) => setState(() => formants = value ?? false),
          ),
          const Text(
            'Renders from the original stems. Applying 0 restores the originals.',
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          ref
              .read(studioProvider.notifier)
              .applyPitch(semitones.round(), formants);
          Navigator.pop(context);
        },
        child: const Text('Apply pitch'),
      ),
    ],
  );
}

class ExportDialog extends ConsumerStatefulWidget {
  const ExportDialog({super.key});
  @override
  ConsumerState<ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends ConsumerState<ExportDialog> {
  String format = 'wav';
  int rate = 44100, channels = 2, bits = 24;
  bool normalize = false;
  final selected = <String>{};
  @override
  void initState() {
    super.initState();
    selected.addAll(
      ref.read(studioProvider).session!.tracks.map((t) => t.stemType),
    );
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Export stems'),
    content: SizedBox(
      width: 420,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Exports each selected stem with its applied pitch. Listening gain, mute, solo, pan and master do not affect export.',
            ),
            for (final key in ref
                .read(studioProvider)
                .session!
                .tracks
                .map((t) => t.stemType))
              CheckboxListTile(
                dense: true,
                title: Text(stemNames[key] ?? key),
                value: selected.contains(key),
                onChanged:
                    (value) => setState(() {
                      if (value == true) {
                        selected.add(key);
                      } else {
                        selected.remove(key);
                      }
                    }),
              ),
            Wrap(
              spacing: 12,
              children: [
                DropdownButton<String>(
                  value: format,
                  items:
                      ['wav', 'flac', 'mp3']
                          .map(
                            (v) => DropdownMenuItem(
                              value: v,
                              child: Text(v.toUpperCase()),
                            ),
                          )
                          .toList(),
                  onChanged: (v) => setState(() => format = v!),
                ),
                DropdownButton<int>(
                  value: rate,
                  items:
                      [44100, 48000]
                          .map(
                            (v) => DropdownMenuItem(
                              value: v,
                              child: Text('$v Hz'),
                            ),
                          )
                          .toList(),
                  onChanged: (v) => setState(() => rate = v!),
                ),
                DropdownButton<int>(
                  value: channels,
                  items: const [
                    DropdownMenuItem(value: 1, child: Text('Mono')),
                    DropdownMenuItem(value: 2, child: Text('Stereo')),
                  ],
                  onChanged: (v) => setState(() => channels = v!),
                ),
                DropdownButton<int>(
                  value: bits,
                  items:
                      [16, 24]
                          .map(
                            (v) => DropdownMenuItem(
                              value: v,
                              child: Text('$v bit'),
                            ),
                          )
                          .toList(),
                  onChanged:
                      format == 'mp3' ? null : (v) => setState(() => bits = v!),
                ),
              ],
            ),
            if (format == 'mp3') const Text('MP3 · 320 kbps'),
            CheckboxListTile(
              title: const Text('Normalize loudness'),
              value: normalize,
              onChanged: (v) => setState(() => normalize = v ?? false),
            ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed:
            selected.isEmpty
                ? null
                : () async {
                  final directory = await FilePicker.platform.getDirectoryPath(
                    dialogTitle: 'Export destination',
                  );
                  if (directory != null && context.mounted) {
                    ref
                        .read(studioProvider.notifier)
                        .export(
                          directory,
                          selected.toList(),
                          format,
                          rate,
                          channels,
                          bits,
                          normalize,
                        );
                    Navigator.pop(context);
                  }
                },
        child: const Text('Choose folder and export'),
      ),
    ],
  );
}
