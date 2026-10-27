part of 'studio_panels.dart';

class StudioTracks extends ConsumerWidget {
  const StudioTracks({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final zoom = ref.watch(timelineZoomProvider);
    final tracks = state.session!.tracks;
    final muted = state.tracks.values.where((track) => track.mute).length;
    return Column(
      children: [
        SizedBox(
          height: 48,
          child: Row(
            children: [
              const Text(
                'DAW mixer',
                style: TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(width: 12),
              _Badge('${tracks.length} STEMS'),
              if (muted > 0) ...[
                const SizedBox(width: 12),
                Text(
                  '$muted muted',
                  style: const TextStyle(fontSize: 11, color: AppColors.muted),
                ),
              ],
              const Spacer(),
              IconButton(
                tooltip: 'Zoom out',
                icon: const Icon(Icons.zoom_out, size: 18),
                onPressed:
                    zoom > 1
                        ? () =>
                            ref.read(timelineZoomProvider.notifier).state =
                                zoom - 1
                        : null,
              ),
              DropdownButton<double>(
                value: zoom,
                underline: const SizedBox.shrink(),
                style: const TextStyle(
                  fontFamily: 'monospace',
                  fontSize: 11,
                  color: AppColors.text,
                ),
                items: [
                  for (final value in [1.0, 2.0, 3.0, 4.0])
                    DropdownMenuItem(
                      value: value,
                      child: Text('${(value * 100).round()}%'),
                    ),
                ],
                onChanged: (value) {
                  if (value != null) {
                    ref.read(timelineZoomProvider.notifier).state = value;
                  }
                },
              ),
              IconButton(
                tooltip: 'Zoom in',
                icon: const Icon(Icons.zoom_in, size: 18),
                onPressed:
                    zoom < 4
                        ? () =>
                            ref.read(timelineZoomProvider.notifier).state =
                                zoom + 1
                        : null,
              ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            clipBehavior: Clip.antiAlias,
            decoration: BoxDecoration(
              border: Border.all(color: AppColors.line),
              borderRadius: BorderRadius.circular(5),
            ),
            child: LayoutBuilder(
              builder: (context, box) {
                final controlWidth = box.maxWidth < 900 ? 240.0 : 298.0;
                final rowHeight = math.max(
                  94.0,
                  (box.maxHeight - 32) / tracks.length,
                );
                // A single vertical scroller keeps controls and waveforms sample aligned.
                // Only the waveform column participates in horizontal timeline zoom.
                return SingleChildScrollView(
                  child: SizedBox(
                    height: 32 + tracks.length * rowHeight,
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        SizedBox(
                          width: controlWidth,
                          child: Column(
                            children: [
                              Container(
                                height: 32,
                                color: AppColors.panel,
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 13,
                                ),
                                child: const Row(
                                  children: [
                                    Text(
                                      'TRACK / MIX',
                                      style: TextStyle(
                                        fontSize: 9,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                    Spacer(),
                                    Text(
                                      'PEAK',
                                      style: TextStyle(
                                        fontSize: 9,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              for (var i = 0; i < tracks.length; i++)
                                SizedBox(
                                  height: rowHeight,
                                  child: _TrackControls(
                                    key: ValueKey(
                                      'controls-${tracks[i].stemType}',
                                    ),
                                    track: tracks[i],
                                    index: i,
                                  ),
                                ),
                            ],
                          ),
                        ),
                        const VerticalDivider(width: 1),
                        Expanded(
                          child: SingleChildScrollView(
                            key: const ValueKey('wave-scroll'),
                            scrollDirection: Axis.horizontal,
                            child: SizedBox(
                              width: (box.maxWidth - controlWidth - 1) * zoom,
                              child: Column(
                                children: [
                                  SizedBox(
                                    height: 32,
                                    width: double.infinity,
                                    child: CustomPaint(
                                      painter: TimelineRulerPainter(
                                        state.duration,
                                      ),
                                    ),
                                  ),
                                  for (final track in tracks)
                                    SizedBox(
                                      height: rowHeight,
                                      child: _TrackWave(
                                        key: ValueKey('wave-${track.stemType}'),
                                        track: track,
                                      ),
                                    ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      ],
    );
  }
}

class _Badge extends StatelessWidget {
  const _Badge(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
    decoration: BoxDecoration(
      color: AppColors.panelRaised,
      borderRadius: BorderRadius.circular(3),
    ),
    child: Text(
      text,
      style: const TextStyle(fontSize: 9, color: AppColors.muted),
    ),
  );
}

class _TrackControls extends ConsumerWidget {
  const _TrackControls({super.key, required this.track, required this.index});
  final native.UiStemTrackDto track;
  final int index;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final controller = ref.read(studioProvider.notifier);
    final mix = state.tracks[track.stemType] ?? const MixTrackState();
    final name = stemNames[track.stemType] ?? track.stemType;
    final color = stemColor(track.stemType);
    final enabled = !state.busy;
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 7, 12, 5),
      decoration: const BoxDecoration(
        color: AppColors.panel,
        border: Border(top: BorderSide(color: AppColors.line)),
      ),
      child: Column(
        children: [
          Row(
            children: [
              Text(
                (index + 1).toString().padLeft(2, '0'),
                style: const TextStyle(fontSize: 9, color: AppColors.muted),
              ),
              const SizedBox(width: 8),
              Icon(stemIcon(track.stemType), color: color, size: 17),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  name,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Expanded(
            child: Row(
              children: [
                _MixToggle(
                  label: 'M',
                  tooltip: 'Mute $name',
                  selected: mix.mute,
                  onTap:
                      enabled
                          ? () => controller.setTrack(
                            track.stemType,
                            mix.copyWith(mute: !mix.mute),
                          )
                          : null,
                ),
                const SizedBox(width: 4),
                _MixToggle(
                  label: 'S',
                  tooltip: 'Solo $name',
                  selected: mix.solo,
                  onTap:
                      enabled
                          ? () => controller.setTrack(
                            track.stemType,
                            mix.copyWith(solo: !mix.solo),
                          )
                          : null,
                ),
                const SizedBox(width: 6),
                SizedBox(
                  width: 46,
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Icon(Icons.surround_sound_outlined, size: 17),
                      SizedBox(
                        height: 20,
                        child: SliderTheme(
                          data: SliderTheme.of(context).copyWith(
                            trackHeight: 1,
                            thumbShape: const RoundSliderThumbShape(
                              enabledThumbRadius: 3,
                            ),
                            overlayShape: SliderComponentShape.noOverlay,
                          ),
                          child: Slider(
                            value: mix.pan.clamp(-1, 1),
                            min: -1,
                            max: 1,
                            semanticFormatterCallback:
                                (value) =>
                                    '$name pan ${value.toStringAsFixed(2)}',
                            onChanged:
                                enabled
                                    ? (v) => controller.setTrack(
                                      track.stemType,
                                      mix.copyWith(pan: v),
                                    )
                                    : null,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                Expanded(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      SliderTheme(
                        data: SliderTheme.of(context).copyWith(
                          activeTrackColor: color,
                          overlayShape: SliderComponentShape.noOverlay,
                        ),
                        child: SizedBox(
                          height: 22,
                          child: Slider(
                            value: mix.gainDb.clamp(-60, 12),
                            min: -60,
                            max: 12,
                            semanticFormatterCallback:
                                (v) => '$name gain ${v.toStringAsFixed(1)} dB',
                            onChanged:
                                enabled
                                    ? (v) => controller.setTrack(
                                      track.stemType,
                                      mix.copyWith(gainDb: v),
                                    )
                                    : null,
                          ),
                        ),
                      ),
                      Text(
                        '${mix.gainDb.toStringAsFixed(1)} dB',
                        style: const TextStyle(
                          fontSize: 9,
                          color: AppColors.muted,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 10),
                SizedBox(
                  width: 38,
                  child: TrackLevelMeter(
                    stemType: track.stemType,
                    peak: state.trackPeaks[track.stemType] ?? 0,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _MixToggle extends StatelessWidget {
  const _MixToggle({
    required this.label,
    required this.tooltip,
    required this.selected,
    this.onTap,
  });
  final String label, tooltip;
  final bool selected;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) => Tooltip(
    message: tooltip,
    child: Semantics(
      button: true,
      toggled: selected,
      label: tooltip,
      child: SizedBox(
        width: 24,
        height: 28,
        child: OutlinedButton(
          onPressed: onTap,
          style: OutlinedButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: Size.zero,
            backgroundColor:
                selected
                    ? AppColors.amber.withValues(alpha: .2)
                    : AppColors.panelRaised,
            foregroundColor: selected ? AppColors.amber : AppColors.muted,
            side: BorderSide(
              color: selected ? AppColors.amber : AppColors.line,
            ),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(3),
            ),
          ),
          child: Text(label, style: const TextStyle(fontSize: 10)),
        ),
      ),
    ),
  );
}

class _TrackWave extends ConsumerWidget {
  const _TrackWave({super.key, required this.track});
  final native.UiStemTrackDto track;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final mix = state.tracks[track.stemType] ?? const MixTrackState();
    final soloActive = state.tracks.values.any((t) => t.solo);
    final audible = !mix.mute && (!soloActive || mix.solo);
    final color = stemColor(track.stemType);
    return LayoutBuilder(
      builder:
          (context, box) => GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown:
                !state.busy
                    ? (details) => ref
                        .read(studioProvider.notifier)
                        .seek(
                          (details.localPosition.dx / box.maxWidth).clamp(
                                0,
                                1,
                              ) *
                              state.duration,
                        )
                    : null,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: color.withValues(alpha: .035),
                border: Border(
                  top: BorderSide(color: color.withValues(alpha: .6)),
                ),
              ),
              child: Stack(
                children: [
                  Positioned.fill(
                    child: CustomPaint(
                      painter: WaveformPainter(
                        peaks: track.peaks,
                        color: color.withValues(alpha: audible ? .9 : .24),
                        progress:
                            state.duration > 0
                                ? state.position / state.duration
                                : 0,
                      ),
                    ),
                  ),
                  Positioned(
                    left: 9,
                    top: 4,
                    child: Text(
                      track.filePath.split(RegExp(r'[/\\]')).last,
                      style: TextStyle(
                        fontSize: 8,
                        color: color.withValues(alpha: .65),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
    );
  }
}

class TimelineRulerPainter extends CustomPainter {
  TimelineRulerPainter(this.duration);
  final double duration;
  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = AppColors.panel);
    final text = TextPainter(textDirection: TextDirection.ltr);
    for (var i = 0; i <= 8; i++) {
      final x = size.width * i / 8;
      text.text = TextSpan(
        text: formatStudioTime(duration * i / 8),
        style: const TextStyle(
          fontFamily: 'monospace',
          fontSize: 9,
          color: AppColors.muted,
        ),
      );
      text.layout();
      text.paint(
        canvas,
        Offset(x.clamp(6, math.max(6, size.width - text.width - 6)), 7),
      );
      canvas.drawLine(
        Offset(x, 26),
        Offset(x, 32),
        Paint()..color = AppColors.line,
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
    final grid = Paint()..color = AppColors.line.withValues(alpha: .55);
    for (var i = 0; i <= 16; i++) {
      final x = size.width * i / 16;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
    }
    canvas.drawLine(
      Offset(0, size.height / 2),
      Offset(size.width, size.height / 2),
      grid,
    );
    final count = peaks.length ~/ 2;
    final path = Path();
    // Peaks are combined min/max buckets from the engine, not separate stereo channels.
    for (var i = 0; i < count; i++) {
      final x = i * size.width / math.max(1, count - 1);
      final y = size.height * (.5 - peaks[i * 2 + 1].clamp(-1, 1) * .39);
      if (i == 0) {
        path.moveTo(x, y);
      } else {
        path.lineTo(x, y);
      }
    }
    for (var i = count - 1; i >= 0; i--) {
      path.lineTo(
        i * size.width / math.max(1, count - 1),
        size.height * (.5 - peaks[i * 2].clamp(-1, 1) * .39),
      );
    }
    if (count > 0) {
      path.close();
      canvas.drawPath(path, Paint()..color = color);
    }
    final x = progress.clamp(0, 1) * size.width;
    canvas.drawLine(
      Offset(x, 0),
      Offset(x, size.height),
      Paint()
        ..color = AppColors.playhead
        ..strokeWidth = 1.5,
    );
    canvas.drawPath(
      Path()
        ..moveTo(x - 4, 0)
        ..lineTo(x + 4, 0)
        ..lineTo(x, 6)
        ..close(),
      Paint()..color = AppColors.playhead,
    );
  }

  @override
  bool shouldRepaint(WaveformPainter old) =>
      old.peaks != peaks || old.progress != progress || old.color != color;
}
