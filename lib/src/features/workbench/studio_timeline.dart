part of 'studio_panels.dart';

class StudioTracks extends ConsumerStatefulWidget {
  const StudioTracks({super.key});

  @override
  ConsumerState<StudioTracks> createState() => _StudioTracksState();
}

class _StudioTracksState extends ConsumerState<StudioTracks> {
  final _desktopScrollController = ScrollController();

  @override
  void dispose() {
    _desktopScrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(
      studioProvider.select(
        (s) => (session: s.session, tracks: s.tracks, duration: s.duration),
      ),
    );
    final zoom = ref.watch(timelineZoomProvider);
    final tracks = state.session?.tracks;
    if (tracks == null) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'Separa un audio para mezclar los cinco stems.',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    final muted = state.tracks.values.where((track) => track.mute).length;

    if (isPhoneLayout(context)) {
      return Column(
        children: [
          _MobileTracksHeader(
            zoom: zoom,
            muted: muted,
            tracksCount: tracks.length,
          ),
          const Expanded(child: MobileStudioTracks()),
        ],
      );
    }

    return Column(
      children: [
        SizedBox(
          height: 48,
          child: Row(
            children: [
              const Text(
                'MEZCLADOR',
                style: TextStyle(fontSize: 10, fontWeight: FontWeight.w600),
              ),
              const SizedBox(width: 12),
              _Badge('${tracks.length} stems sincronizados'),
              if (muted > 0) ...[
                const SizedBox(width: 12),
                Text(
                  '$muted silenciadas',
                  style: const TextStyle(fontSize: 11, color: AppColors.muted),
                ),
              ],
              const Spacer(),
              IconButton(
                key: const ValueKey('zoom-out'),
                tooltip: 'Alejar',
                style: IconButton.styleFrom(minimumSize: const Size(48, 48)),
                icon: const Icon(Icons.zoom_out, size: 18),
                onPressed:
                    zoom > 1
                        ? () =>
                            ref.read(timelineZoomProvider.notifier).state =
                                (zoom - 1).clamp(1.0, 8.0)
                        : null,
              ),
              DropdownButton<double>(
                value: [1.0, 2.0, 3.0, 4.0].contains(zoom) ? zoom : null,
                hint: Text('${(zoom * 100).round()}%'),
                underline: const SizedBox.shrink(),
                style: const TextStyle(
                  fontFamily: 'Inter',
                  fontSize: 11,
                  color: AppColors.text,
                ),
                items: [
                  for (final value in [1.0, 2.0, 3.0, 4.0, 6.0, 8.0])
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
                key: const ValueKey('zoom-in'),
                tooltip: 'Acercar',
                style: IconButton.styleFrom(minimumSize: const Size(48, 48)),
                icon: const Icon(Icons.zoom_in, size: 18),
                onPressed:
                    zoom < 8
                        ? () =>
                            ref.read(timelineZoomProvider.notifier).state =
                                (zoom + 1).clamp(1.0, 8.0)
                        : null,
              ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            clipBehavior: Clip.antiAlias,
            decoration: BoxDecoration(borderRadius: BorderRadius.circular(12)),
            child: LayoutBuilder(
              builder: (context, box) {
                final controlWidth = box.maxWidth < 1050 ? 300.0 : 436.0;
                final rowHeight = math.max(
                  106.0 * ref.watch(studioDensityProvider),
                  (box.maxHeight - 32) / tracks.length,
                );
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
                                      'PISTAS',
                                      style: TextStyle(
                                        fontSize: 9,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                    Spacer(),
                                    Text(
                                      'VU',
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
                            controller: _desktopScrollController,
                            scrollDirection: Axis.horizontal,
                            child: SizedBox(
                              width: (box.maxWidth - controlWidth - 1) * zoom,
                              child: Column(
                                children: [
                                  SizedBox(
                                    height: 32,
                                    width: double.infinity,
                                    child: CustomPaint(
                                      painter:
                                          ref.watch(studioGridProvider)
                                              ? TimelineRulerPainter(
                                                state.duration,
                                              )
                                              : null,
                                    ),
                                  ),
                                  for (final track in tracks)
                                    SizedBox(
                                      height: rowHeight,
                                      child: _TrackWave(
                                        key: ValueKey('wave-${track.stemType}'),
                                        track: track,
                                        scrollController:
                                            _desktopScrollController,
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

class _MobileTracksHeader extends ConsumerWidget {
  const _MobileTracksHeader({
    required this.zoom,
    required this.muted,
    required this.tracksCount,
  });
  final double zoom;
  final int muted;
  final int tracksCount;

  @override
  Widget build(BuildContext context, WidgetRef ref) => SizedBox(
    height: 48,
    child: Row(
      children: [
        const Text(
          'MEZCLADOR',
          style: TextStyle(fontSize: 10, fontWeight: FontWeight.w600),
        ),
        const SizedBox(width: 8),
        _Badge('$tracksCount stems'),
        if (muted > 0) ...[
          const SizedBox(width: 8),
          Text(
            '$muted silenciadas',
            style: const TextStyle(fontSize: 9.5, color: AppColors.muted),
          ),
        ],
        const Spacer(),
        IconButton(
          key: const ValueKey('zoom-out'),
          tooltip: 'Alejar',
          style: IconButton.styleFrom(minimumSize: const Size(48, 48)),
          icon: const Icon(Icons.zoom_out, size: 18),
          onPressed:
              zoom > 1
                  ? () =>
                      ref.read(timelineZoomProvider.notifier).state =
                          (zoom - 1).clamp(1.0, 8.0)
                  : null,
        ),
        Text(
          '${(zoom * 100).round()}%',
          style: const TextStyle(
            fontSize: 11,
            color: AppColors.text,
            fontWeight: FontWeight.w600,
          ),
        ),
        IconButton(
          key: const ValueKey('zoom-in'),
          tooltip: 'Acercar',
          style: IconButton.styleFrom(minimumSize: const Size(48, 48)),
          icon: const Icon(Icons.zoom_in, size: 18),
          onPressed:
              zoom < 8
                  ? () =>
                      ref.read(timelineZoomProvider.notifier).state =
                          (zoom + 1).clamp(1.0, 8.0)
                  : null,
        ),
      ],
    ),
  );
}

class MobileStudioTracks extends ConsumerStatefulWidget {
  const MobileStudioTracks({super.key});

  @override
  ConsumerState<MobileStudioTracks> createState() => _MobileStudioTracksState();
}

class _MobileStudioTracksState extends ConsumerState<MobileStudioTracks> {
  final Set<String> _expandedStems = {};

  @override
  void initState() {
    super.initState();
    final tracks = ref.read(studioProvider).session?.tracks;
    if (tracks != null && tracks.isNotEmpty) {
      _expandedStems.add(tracks.first.stemType);
    }
  }

  void _toggleExpand(String stemType) {
    setState(() {
      if (_expandedStems.contains(stemType)) {
        _expandedStems.remove(stemType);
      } else {
        _expandedStems.add(stemType);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(
      studioProvider.select(
        (s) => (session: s.session, tracks: s.tracks, busy: s.busy),
      ),
    );
    final tracks = state.session?.tracks;
    if (tracks == null || tracks.isEmpty) {
      return const SizedBox.shrink();
    }

    return SingleChildScrollView(
      key: const ValueKey('mobile-tracks-scroll'),
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        children: [
          for (var index = 0; index < tracks.length; index++) ...[
            if (index > 0) const SizedBox(height: 8),
            _MobileTrackCard(
              key: ValueKey('mobile-track-${tracks[index].stemType}'),
              track: tracks[index],
              index: index,
              isExpanded: _expandedStems.contains(tracks[index].stemType),
              onToggleExpand: () => _toggleExpand(tracks[index].stemType),
            ),
          ],
        ],
      ),
    );
  }
}

class _MobileTrackCard extends ConsumerStatefulWidget {
  const _MobileTrackCard({
    super.key,
    required this.track,
    required this.index,
    required this.isExpanded,
    required this.onToggleExpand,
  });

  final native.UiStemTrackDto track;
  final int index;
  final bool isExpanded;
  final VoidCallback onToggleExpand;

  @override
  ConsumerState<_MobileTrackCard> createState() => _MobileTrackCardState();
}

class _MobileTrackCardState extends ConsumerState<_MobileTrackCard> {
  late final ScrollController _waveformScrollController;

  @override
  void initState() {
    super.initState();
    _waveformScrollController = ScrollController();
  }

  @override
  void dispose() {
    _waveformScrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final track = widget.track;
    final state = ref.watch(
      studioProvider.select(
        (s) => (
          mix: s.tracks[track.stemType],
          peak: s.trackPeaks[track.stemType] ?? 0.0,
          busy: s.busy,
        ),
      ),
    );
    final controller = ref.read(studioProvider.notifier);
    final mix = state.mix ?? const MixTrackState();
    final name = stemNames[track.stemType] ?? track.stemType;
    final color = stemColor(track.stemType);
    final peak = state.peak;
    final enabled = !state.busy;
    final zoom = ref.watch(timelineZoomProvider);

    return Container(
      decoration: BoxDecoration(
        color: AppColors.panel,
        borderRadius: BorderRadius.circular(10),
        border: Border(left: BorderSide(color: color, width: 4)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            key: ValueKey('header-${track.stemType}'),
            onTap: widget.onToggleExpand,
            borderRadius: BorderRadius.circular(10),
            child: Container(
              height: 52,
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                children: [
                  Icon(stemIcon(track.stemType), color: color, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 12.5,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        Text(
                          '${track.channels == 1 ? "Mono" : "Estéreo"} · ${(track.sampleRate / 1000).toStringAsFixed(track.sampleRate % 1000 == 0 ? 0 : 1)} kHz',
                          style: const TextStyle(
                            fontSize: 8.5,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 4),
                  SizedBox(
                    width: 44,
                    child: TrackLevelMeter(
                      stemType: track.stemType,
                      peak: peak,
                    ),
                  ),
                  const SizedBox(width: 6),
                  _MixToggle(
                    key: ValueKey('mute-${track.stemType}'),
                    label: 'M',
                    tooltip: 'Silenciar $name',
                    selected: mix.mute,
                    minSize: const Size(48, 48),
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
                    key: ValueKey('solo-${track.stemType}'),
                    label: 'S',
                    tooltip: 'Solo $name',
                    selected: mix.solo,
                    minSize: const Size(48, 48),
                    onTap:
                        enabled
                            ? () => controller.setTrack(
                              track.stemType,
                              mix.copyWith(solo: !mix.solo),
                            )
                            : null,
                  ),
                  const SizedBox(width: 2),
                  Icon(
                    widget.isExpanded
                        ? Icons.expand_less
                        : Icons.expand_more,
                    size: 20,
                    color: AppColors.muted,
                  ),
                ],
              ),
            ),
          ),
          if (widget.isExpanded) ...[
            const Divider(height: 1, color: AppColors.line),
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      const Text(
                        'VOLUMEN',
                        style: TextStyle(
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          color: AppColors.muted,
                        ),
                      ),
                      const Spacer(),
                      Text(
                        '${mix.gainDb.toStringAsFixed(1)} dB',
                        style: const TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w600,
                          fontFeatures: [FontFeature.tabularFigures()],
                          color: AppColors.text,
                        ),
                      ),
                    ],
                  ),
                  SizedBox(
                    height: 48,
                    child: SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        activeTrackColor: color,
                        overlayShape: SliderComponentShape.noOverlay,
                        thumbShape: const RoundSliderThumbShape(
                          enabledThumbRadius: 7,
                        ),
                      ),
                      child: Slider(
                        key: ValueKey('volume-slider-${track.stemType}'),
                        value: mix.gainDb.clamp(-60, 12),
                        min: -60,
                        max: 12,
                        semanticFormatterCallback:
                            (v) =>
                                '$name ganancia ${v.toStringAsFixed(1)} dB',
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
                  Row(
                    children: [
                      const Text(
                        'PANEO',
                        style: TextStyle(
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          color: AppColors.muted,
                        ),
                      ),
                      const Spacer(),
                      Text(
                        mix.pan == 0
                            ? 'Centro'
                            : mix.pan < 0
                            ? 'L ${(mix.pan.abs() * 100).round()}%'
                            : 'R ${(mix.pan * 100).round()}%',
                        style: const TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w600,
                          fontFeatures: [FontFeature.tabularFigures()],
                          color: AppColors.text,
                        ),
                      ),
                    ],
                  ),
                  SizedBox(
                    height: 48,
                    child: SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        overlayShape: SliderComponentShape.noOverlay,
                        thumbShape: const RoundSliderThumbShape(
                          enabledThumbRadius: 7,
                        ),
                      ),
                      child: Slider(
                        key: ValueKey('pan-slider-${track.stemType}'),
                        value: mix.pan.clamp(-1, 1),
                        min: -1,
                        max: 1,
                        semanticFormatterCallback:
                            (v) =>
                                '$name paneo ${v.toStringAsFixed(2)}',
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
                  const SizedBox(height: 4),
                  LayoutBuilder(
                    builder:
                        (context, box) => SizedBox(
                          height: 84,
                          child: SingleChildScrollView(
                            key: ValueKey(
                              'wave-scroll-${track.stemType}',
                            ),
                            controller: _waveformScrollController,
                            scrollDirection: Axis.horizontal,
                            physics: const NeverScrollableScrollPhysics(),
                            child: SizedBox(
                              width: math.max(
                                box.maxWidth,
                                box.maxWidth * zoom,
                              ),
                              child: _TrackWave(
                                key: ValueKey('wave-${track.stemType}'),
                                track: track,
                                scrollController:
                                    _waveformScrollController,
                              ),
                            ),
                          ),
                        ),
                  ),
                ],
              ),
            ),
          ],
        ],
      ),
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
    final state = ref.watch(
      studioProvider.select(
        (s) => (
          mix: s.tracks[track.stemType],
          peak: s.trackPeaks[track.stemType] ?? 0.0,
          busy: s.busy,
        ),
      ),
    );
    final controller = ref.read(studioProvider.notifier);
    final mix = state.mix ?? const MixTrackState();
    final name = stemNames[track.stemType] ?? track.stemType;
    final color = stemColor(track.stemType);
    final peak = state.peak;
    final enabled = !state.busy;
    return Container(
      margin: const EdgeInsets.only(bottom: 8),
      padding: const EdgeInsets.fromLTRB(12, 8, 14, 6),
      decoration: BoxDecoration(
        color: AppColors.panel,
        borderRadius: const BorderRadius.horizontal(left: Radius.circular(12)),
        border: Border(left: BorderSide(color: color, width: 3)),
      ),
      child: Column(
        children: [
          Row(
            children: [
              Text(
                (index + 1).toString().padLeft(2, '0'),
                style: const TextStyle(fontSize: 9.6, color: AppColors.muted),
              ),
              const SizedBox(width: 14),
              Icon(stemIcon(track.stemType), color: color, size: 21),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  name,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 12.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
          Align(
            alignment: Alignment.centerLeft,
            child: Padding(
              padding: const EdgeInsets.only(left: 53, top: 2),
              child: Text(
                '${track.channels == 1 ? "Mono" : "Estéreo"} · ${(track.sampleRate / 1000).toStringAsFixed(track.sampleRate % 1000 == 0 ? 0 : 1)} kHz',
                style: const TextStyle(fontSize: 8.6, color: AppColors.muted),
              ),
            ),
          ),
          const SizedBox(height: 4),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                _MixToggle(
                  label: 'M',
                  tooltip: 'Silenciar $name',
                  selected: mix.mute,
                  onTap:
                      enabled
                          ? () => controller.setTrack(
                            track.stemType,
                            mix.copyWith(mute: !mix.mute),
                          )
                          : null,
                ),
                const SizedBox(width: 6),
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
                const SizedBox(width: 10),
                SizedBox(
                  width: 48,
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      const Text(
                        'PAN',
                        style: TextStyle(
                          fontSize: 7.8,
                          height: 1,
                          color: AppColors.muted,
                        ),
                      ),
                      SizedBox(
                        height: 19,
                        child: SliderTheme(
                          data: SliderTheme.of(context).copyWith(
                            overlayShape: SliderComponentShape.noOverlay,
                            thumbShape: const RoundSliderThumbShape(
                              enabledThumbRadius: 3,
                            ),
                          ),
                          child: Slider(
                            value: mix.pan.clamp(-1, 1),
                            min: -1,
                            max: 1,
                            semanticFormatterCallback:
                                (v) => '$name paneo ${v.toStringAsFixed(2)}',
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
                      const Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Text(
                            'L',
                            style: TextStyle(
                              fontSize: 7,
                              height: 1,
                              color: AppColors.muted,
                            ),
                          ),
                          Text(
                            'R',
                            style: TextStyle(
                              fontSize: 7,
                              height: 1,
                              color: AppColors.muted,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'VOL  ${mix.gainDb.toStringAsFixed(1)} dB',
                        style: const TextStyle(
                          fontSize: 8,
                          height: 1,
                          color: AppColors.muted,
                        ),
                      ),
                      SizedBox(
                        height: 22,
                        child: SliderTheme(
                          data: SliderTheme.of(context).copyWith(
                            activeTrackColor: color,
                            overlayShape: SliderComponentShape.noOverlay,
                          ),
                          child: Slider(
                            value: mix.gainDb.clamp(-60, 12),
                            min: -60,
                            max: 12,
                            semanticFormatterCallback:
                                (v) =>
                                    '$name ganancia ${v.toStringAsFixed(1)} dB',
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
                    ],
                  ),
                ),
                const SizedBox(width: 14),
                SizedBox(
                  width: 68,
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'VU  ${peak > 0 ? (20 * math.log(peak) / math.ln10).toStringAsFixed(1) : "−∞"} dB',
                        style: const TextStyle(
                          fontSize: 8,
                          height: 1,
                          color: AppColors.muted,
                        ),
                      ),
                      const SizedBox(height: 9),
                      TrackLevelMeter(stemType: track.stemType, peak: peak),
                    ],
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
    super.key,
    required this.label,
    required this.tooltip,
    required this.selected,
    this.minSize,
    this.onTap,
  });
  final String label, tooltip;
  final bool selected;
  final Size? minSize;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) => Tooltip(
    message: tooltip,
    child: Semantics(
      button: true,
      toggled: selected,
      label: tooltip,
      child: SizedBox(
        width: minSize?.width ?? 24,
        height: minSize?.height ?? 28,
        child: OutlinedButton(
          onPressed: onTap,
          style: OutlinedButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: minSize ?? Size.zero,
            backgroundColor:
                selected
                    ? AppColors.amber.withValues(alpha: .2)
                    : AppColors.panelRaised,
            foregroundColor: selected ? AppColors.amber : AppColors.muted,
            side: BorderSide(
              color: selected ? AppColors.amber : AppColors.line,
            ),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(minSize != null ? 8 : 3),
            ),
          ),
          child: Text(
            label,
            style: TextStyle(
              fontSize: minSize != null ? 14 : 10,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
      ),
    ),
  );
}

class _TrackWave extends ConsumerStatefulWidget {
  const _TrackWave({
    super.key,
    required this.track,
    this.scrollController,
  });
  final native.UiStemTrackDto track;
  final ScrollController? scrollController;

  @override
  ConsumerState<_TrackWave> createState() => _TrackWaveState();
}

class _TrackWaveState extends ConsumerState<_TrackWave> {
  double _startZoom = 1.0;
  Offset _lastFocalPoint = Offset.zero;

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(
      studioProvider.select(
        (s) => (
          duration: s.duration,
          position: s.position,
          busy: s.busy,
          sessionId: s.session?.id,
          tracks: s.tracks,
        ),
      ),
    );
    final zoom = ref.watch(timelineZoomProvider);
    final detail =
        state.sessionId == null
            ? null
            : ref
                .watch(
                  waveformDetailProvider((
                    sessionId: state.sessionId!,
                    stemType: widget.track.stemType,
                    filePath: widget.track.filePath,
                    buckets: (1000 * zoom).round(),
                  )),
                )
                .valueOrNull;
    final peaks =
        detail != null && detail.isNotEmpty ? detail : widget.track.peaks;
    final mix = state.tracks[widget.track.stemType] ?? const MixTrackState();
    final soloActive = state.tracks.values.any((t) => t.solo);
    final audible = !mix.mute && (!soloActive || mix.solo);
    final color = stemColor(widget.track.stemType);
    return LayoutBuilder(
      builder:
          (context, box) => GestureDetector(
            behavior: HitTestBehavior.opaque,
            onScaleStart: (details) {
              _startZoom = ref.read(timelineZoomProvider);
              _lastFocalPoint = details.focalPoint;
            },
            onScaleUpdate: (details) {
              if (details.pointerCount >= 2) {
                final newZoom =
                    (_startZoom * details.horizontalScale).clamp(1.0, 8.0);
                ref.read(timelineZoomProvider.notifier).state = newZoom;
              } else if (details.pointerCount == 1 &&
                  widget.scrollController != null) {
                final delta = details.focalPoint.dx - _lastFocalPoint.dx;
                _lastFocalPoint = details.focalPoint;
                if (widget.scrollController!.hasClients && delta != 0) {
                  final newOffset =
                      (widget.scrollController!.offset - delta).clamp(
                        0.0,
                        widget.scrollController!.position.maxScrollExtent,
                      );
                  widget.scrollController!.jumpTo(newOffset);
                }
              }
            },
            onTapDown:
                !state.busy
                    ? (details) => ref
                        .read(studioProvider.notifier)
                        .seek(
                          (details.localPosition.dx / box.maxWidth).clamp(
                                0.0,
                                1.0,
                              ) *
                              state.duration,
                        )
                    : null,
            child: Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: const Color(0xFF0F161E),
                  borderRadius: const BorderRadius.horizontal(
                    right: Radius.circular(12),
                  ),
                  border: Border(
                    top: const BorderSide(color: Color(0xFF1A2531)),
                  ),
                ),
                child: Stack(
                  children: [
                    Positioned.fill(
                      child: RepaintBoundary(
                        child: CustomPaint(
                          painter: WaveformPainter(
                            peaks: peaks,
                            color: color.withValues(alpha: audible ? .9 : .24),
                            progress: 0,
                            showPlayhead: false,
                            showGrid: ref.watch(studioGridProvider),
                          ),
                        ),
                      ),
                    ),
                    Positioned.fill(
                      child: RepaintBoundary(
                        child: CustomPaint(
                          painter: PlayheadPainter(
                            state.duration > 0
                                ? state.position / state.duration
                                : 0,
                          ),
                        ),
                      ),
                    ),
                    Positioned(
                      left: 9,
                      top: 4,
                      child: Text(
                        widget.track.filePath.split(RegExp(r'[/\\]')).last,
                        style: TextStyle(
                          fontSize: 8,
                          height: 1,
                          color: color.withValues(alpha: .65),
                        ),
                      ),
                    ),
                  ],
                ),
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
    final safeDuration = duration.isFinite && duration > 0 ? duration : 0.0;
    final desiredStep = safeDuration / 7;
    final step =
        [
          1.0,
          2.0,
          5.0,
          10.0,
          15.0,
          30.0,
          60.0,
          120.0,
          300.0,
          600.0,
          1800.0,
          3600.0,
        ].where((value) => value >= desiredStep).firstOrNull ??
        (desiredStep / 3600).ceil() * 3600.0;
    final ticks = [
      for (var time = 0.0; time < safeDuration; time += step) time,
      safeDuration,
    ];
    for (final time in ticks) {
      final x = safeDuration > 0 ? size.width * time / safeDuration : 0.0;
      text.text = TextSpan(
        text:
            '${time.floor() ~/ 60}:${(time.floor() % 60).toString().padLeft(2, '0')}',
        style: const TextStyle(
          fontFamily: 'Inter',
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
    this.showGrid = true,
    this.showPlayhead = true,
  });
  final List<double> peaks;
  final Color color;
  final double progress;
  final bool showGrid;
  final bool showPlayhead;
  @override
  void paint(Canvas canvas, Size size) {
    final grid = Paint()..color = AppColors.line.withValues(alpha: .55);
    for (var i = 0; showGrid && i <= 16; i++) {
      final x = size.width * i / 16;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
    }
    if (showGrid) {
      canvas.drawLine(
        Offset(0, size.height / 2),
        Offset(size.width, size.height / 2),
        grid,
      );
    }
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
    if (showPlayhead) PlayheadPainter(progress).paint(canvas, size);
  }

  @override
  bool shouldRepaint(WaveformPainter old) =>
      !identical(old.peaks, peaks) ||
      old.color != color ||
      old.showGrid != showGrid ||
      old.showPlayhead != showPlayhead ||
      (showPlayhead && old.progress != progress);
}

class PlayheadPainter extends CustomPainter {
  PlayheadPainter(this.progress);
  final double progress;
  @override
  void paint(Canvas canvas, Size size) {
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
  bool shouldRepaint(PlayheadPainter old) => old.progress != progress;
}
