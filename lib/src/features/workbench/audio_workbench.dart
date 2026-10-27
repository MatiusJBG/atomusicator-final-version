import 'package:file_picker/file_picker.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/di/workbench_providers.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/theme/app_theme.dart';

class AudioWorkbench extends ConsumerStatefulWidget {
  const AudioWorkbench({super.key});

  @override
  ConsumerState<AudioWorkbench> createState() => _AudioWorkbenchState();
}

class _AudioWorkbenchState extends ConsumerState<AudioWorkbench> {
  bool _dragging = false;

  Future<void> _importAudio() async {
    final selection = await FilePicker.platform.pickFiles(
      type: FileType.audio,
      allowMultiple: true,
      withData: false,
    );
    if (selection == null) return;
    _addAudio([
      for (final file in selection.files)
        if (file.path != null)
          SelectedAudioFile(
            path: file.path!,
            name: file.name,
            sizeBytes: file.size,
          ),
    ]);
  }

  static const _audioExtensions = {
    'wav',
    'mp3',
    'flac',
    'ogg',
    'm4a',
    'aac',
    'aiff',
    'aif',
    'opus',
    'wma',
  };

  void _addAudio(List<SelectedAudioFile> files) {
    final accepted =
        files
            .where(
              (file) => _audioExtensions.contains(
                file.name.split('.').last.toLowerCase(),
              ),
            )
            .toList();
    if (accepted.isNotEmpty) {
      ref.read(workbenchProvider.notifier).addSources(accepted);
    }
    final rejected = files.length - accepted.length;
    if (rejected > 0 && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('$rejected unsupported file(s) skipped.')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final workbench = ref.watch(workbenchProvider);
    final engineStatus = ref.watch(nativeEngineStatusProvider);
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 980;
        return Scaffold(
          body: SafeArea(
            child: Row(
              children: [
                _ToolRail(compact: compact),
                const VerticalDivider(width: 1, thickness: 1),
                Expanded(
                  child: Column(
                    children: [
                      _TopBar(onImport: _importAudio),
                      const Divider(height: 1),
                      Expanded(
                        child: _WorkbenchBody(
                          sources: workbench.sources,
                          compact: compact,
                          onImport: _importAudio,
                          onDrop:
                              (paths) => _addAudio([
                                for (final path in paths)
                                  SelectedAudioFile(
                                    path: path,
                                    name: path.split(RegExp(r'[/\\]')).last,
                                    sizeBytes: 0,
                                  ),
                              ]),
                          onRemove:
                              ref.read(workbenchProvider.notifier).removeSource,
                          dragging: _dragging,
                          onDragChanged:
                              (value) => setState(() => _dragging = value),
                          engineStatus: engineStatus,
                        ),
                      ),
                      _TransportBar(
                        hasAudio: workbench.hasSources,
                        compact: compact,
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _ToolRail extends StatelessWidget {
  const _ToolRail({required this.compact});
  final bool compact;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: compact ? 56 : 72,
      child: Column(
        children: [
          const SizedBox(height: 19),
          Container(
            width: 38,
            height: 38,
            decoration: BoxDecoration(
              color: AppColors.panelRaised,
              borderRadius: BorderRadius.circular(11),
            ),
            child: const Icon(
              Icons.graphic_eq_rounded,
              color: AppColors.seaGlass,
              size: 21,
            ),
          ),
          const SizedBox(height: 34),
          const _RailButton(
            icon: Icons.tune_rounded,
            label: 'Workbench',
            selected: true,
          ),
          const _RailButton(
            icon: Icons.library_music_outlined,
            label: 'Sessions',
          ),
          const _RailButton(icon: Icons.file_upload_outlined, label: 'Exports'),
          const Spacer(),
          const _RailButton(icon: Icons.settings_outlined, label: 'Settings'),
          const SizedBox(height: 12),
        ],
      ),
    );
  }
}

class _RailButton extends StatelessWidget {
  const _RailButton({
    required this.icon,
    required this.label,
    this.selected = false,
  });
  final IconData icon;
  final String label;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Tooltip(
        message: label,
        child: InkWell(
          onTap: null,
          borderRadius: BorderRadius.circular(10),
          child: Container(
            width: 44,
            height: 44,
            decoration: BoxDecoration(
              color: selected ? AppColors.panelRaised : Colors.transparent,
              borderRadius: BorderRadius.circular(10),
            ),
            child: Icon(
              icon,
              color: selected ? AppColors.seaGlass : AppColors.muted,
              size: 20,
            ),
          ),
        ),
      ),
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar({required this.onImport});
  final VoidCallback onImport;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(28, 18, 28, 17),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Workbench',
                  style: Theme.of(context).textTheme.headlineMedium,
                ),
                const SizedBox(height: 3),
                Text(
                  'Separate the parts. Keep the whole.',
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Undo',
            onPressed: null,
            icon: const Icon(Icons.undo_rounded),
          ),
          IconButton(
            tooltip: 'Redo',
            onPressed: null,
            icon: const Icon(Icons.redo_rounded),
          ),
          const SizedBox(width: 7),
          FilledButton.icon(
            onPressed: onImport,
            icon: const Icon(Icons.add_rounded, size: 18),
            label: const Text('Import audio'),
          ),
        ],
      ),
    );
  }
}

class _WorkbenchBody extends StatelessWidget {
  const _WorkbenchBody({
    required this.sources,
    required this.compact,
    required this.onImport,
    required this.onDrop,
    required this.onRemove,
    required this.dragging,
    required this.onDragChanged,
    required this.engineStatus,
  });
  final List<SelectedAudioFile> sources;
  final bool compact;
  final VoidCallback onImport;
  final ValueChanged<List<String>> onDrop;
  final ValueChanged<String> onRemove;
  final bool dragging;
  final ValueChanged<bool> onDragChanged;
  final AsyncValue<NativeEngineStatus> engineStatus;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.fromLTRB(
        compact ? 18 : 28,
        22,
        compact ? 18 : 28,
        18,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _SessionHeader(
            hasSource: sources.isNotEmpty,
            engineStatus: engineStatus,
          ),
          const SizedBox(height: 6),
          engineStatus.when(
            data:
                (status) => _EngineStatusLabel(
                  label:
                      'Native engine connected · ${status.supportedStems.length} stems supported · separation not configured',
                  connected: true,
                ),
            error:
                (_, __) => const _EngineStatusLabel(
                  label:
                      'Native engine unavailable · audio import is still available',
                  connected: false,
                ),
            loading:
                () => const _EngineStatusLabel(
                  label: 'Checking native engine…',
                  connected: false,
                ),
          ),
          const SizedBox(height: 10),
          const _StemStrip(),
          const SizedBox(height: 14),
          Expanded(
            child: DropTarget(
              onDragEntered: (_) => onDragChanged(true),
              onDragExited: (_) => onDragChanged(false),
              onDragDone: (details) {
                onDragChanged(false);
                onDrop(
                  details.files
                      .where((file) => file.path.isNotEmpty)
                      .map((file) => file.path)
                      .toList(),
                );
              },
              child: _TrackCanvas(
                sources: sources,
                onImport: onImport,
                onRemove: onRemove,
                dragging: dragging,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _SessionHeader extends StatelessWidget {
  const _SessionHeader({required this.hasSource, required this.engineStatus});
  final bool hasSource;
  final AsyncValue<NativeEngineStatus> engineStatus;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                hasSource ? 'Untitled session' : 'New session',
                style: Theme.of(context).textTheme.titleLarge,
              ),
              const SizedBox(height: 3),
              Text(
                hasSource ? 'Audio selected' : 'Import a mix to begin',
                style: Theme.of(context).textTheme.bodyMedium,
              ),
            ],
          ),
        ),
        Tooltip(
          message:
              'Separation is unavailable until a verified model and native API are configured.',
          child: OutlinedButton.icon(
            onPressed: null,
            icon: const Icon(Icons.auto_awesome_outlined, size: 16),
            label: const Text('Separate stems'),
          ),
        ),
        const SizedBox(width: 9),
        IconButton(
          tooltip: 'Session options',
          onPressed: null,
          icon: const Icon(Icons.more_horiz_rounded),
        ),
      ],
    );
  }
}

class _EngineStatusLabel extends StatelessWidget {
  const _EngineStatusLabel({required this.label, required this.connected});
  final String label;
  final bool connected;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Icon(
        Icons.circle,
        size: 7,
        color: connected ? AppColors.seaGlass : AppColors.amber,
      ),
      const SizedBox(width: 7),
      Expanded(
        child: Text(
          label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: Theme.of(context).textTheme.labelMedium,
        ),
      ),
    ],
  );
}

class _StemStrip extends StatelessWidget {
  const _StemStrip();

  static const _stems = [
    ('Lead vocal', AppColors.seaGlass),
    ('Backing vocals', AppColors.lavender),
    ('Drums', AppColors.amber),
    ('Bass', AppColors.blue),
    ('Instrumental', AppColors.coral),
  ];

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 76,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        itemCount: _stems.length,
        separatorBuilder: (_, __) => const SizedBox(width: 8),
        itemBuilder: (context, index) {
          final stem = _stems[index];
          return Container(
            width: 166,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: AppColors.panel,
              border: Border.all(color: AppColors.line.withValues(alpha: 0.65)),
              borderRadius: BorderRadius.circular(9),
            ),
            child: Row(
              children: [
                Container(
                  width: 3,
                  height: 34,
                  decoration: BoxDecoration(
                    color: stem.$2,
                    borderRadius: BorderRadius.circular(3),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        stem.$1,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.titleMedium,
                      ),
                      const SizedBox(height: 2),
                      Text(
                        'Not separated',
                        style: Theme.of(context).textTheme.labelMedium,
                      ),
                    ],
                  ),
                ),
                Icon(
                  Icons.circle_outlined,
                  color: AppColors.muted.withValues(alpha: 0.65),
                  size: 14,
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

class _TrackCanvas extends StatelessWidget {
  const _TrackCanvas({
    required this.sources,
    required this.onImport,
    required this.onRemove,
    required this.dragging,
  });
  final List<SelectedAudioFile> sources;
  final VoidCallback onImport;
  final ValueChanged<String> onRemove;
  final bool dragging;

  @override
  Widget build(BuildContext context) {
    return Container(
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: AppColors.panel.withValues(alpha: 0.52),
        border: Border.all(
          color: dragging ? AppColors.seaGlass : AppColors.line,
          width: dragging ? 2 : 1,
        ),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Stack(
        children: [
          Positioned.fill(child: CustomPaint(painter: _TimelineGridPainter())),
          if (sources.isEmpty)
            Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 390),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 48,
                      height: 48,
                      decoration: BoxDecoration(
                        color: AppColors.panelRaised,
                        borderRadius: BorderRadius.circular(16),
                      ),
                      child: const Icon(
                        Icons.graphic_eq_rounded,
                        color: AppColors.seaGlass,
                        size: 26,
                      ),
                    ),
                    const SizedBox(height: 7),
                    Text(
                      'Your mix starts here',
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      dragging
                          ? 'Drop audio files to add them to this session.'
                          : 'Bring in a song to build a five-stem session. Your original file stays where it is.',
                      textAlign: TextAlign.center,
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                    const SizedBox(height: 8),
                    OutlinedButton.icon(
                      onPressed: onImport,
                      icon: const Icon(Icons.folder_open_outlined, size: 17),
                      label: const Text('Choose audio files'),
                    ),
                  ],
                ),
              ),
            )
          else
            ListView.separated(
              padding: const EdgeInsets.fromLTRB(20, 21, 20, 20),
              itemCount: sources.length,
              separatorBuilder: (_, __) => const SizedBox(height: 9),
              itemBuilder:
                  (context, index) => _SourceRow(
                    file: sources[index],
                    onRemove: () => onRemove(sources[index].path),
                  ),
            ),
        ],
      ),
    );
  }
}

class _SourceRow extends StatelessWidget {
  const _SourceRow({required this.file, required this.onRemove});
  final SelectedAudioFile file;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 58,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      decoration: BoxDecoration(
        color: AppColors.panel,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        children: [
          const Icon(
            Icons.audio_file_outlined,
            color: AppColors.seaGlass,
            size: 20,
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              file.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ),
          Text('Selected', style: Theme.of(context).textTheme.labelMedium),
          const SizedBox(width: 12),
          Text(
            _formatBytes(file.sizeBytes),
            style: Theme.of(context).textTheme.labelMedium,
          ),
          IconButton(
            tooltip: 'Remove ${file.name}',
            onPressed: onRemove,
            icon: const Icon(Icons.close_rounded, size: 18),
          ),
        ],
      ),
    );
  }

  String _formatBytes(int bytes) {
    if (bytes == 0) return 'Size unavailable';
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(0)} KB';
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
}

class _TimelineGridPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final line =
        Paint()
          ..color = AppColors.line.withValues(alpha: 0.36)
          ..strokeWidth = 1;
    final label = TextPainter(textDirection: TextDirection.ltr);
    for (var index = 0; index < 9; index++) {
      final x = size.width * index / 8;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), line);
      if (index < 8) {
        label.text = TextSpan(
          text: '${index * 15}s',
          style: const TextStyle(fontSize: 10, color: AppColors.muted),
        );
        label.layout();
        label.paint(canvas, Offset(x + 5, 7));
      }
    }
    for (var index = 0; index < 5; index++) {
      final y = size.height * (index + 1) / 6;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), line);
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class _TransportBar extends StatelessWidget {
  const _TransportBar({required this.hasAudio, required this.compact});
  final bool hasAudio;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 72,
      padding: EdgeInsets.symmetric(horizontal: compact ? 14 : 28),
      decoration: const BoxDecoration(
        color: AppColors.panel,
        border: Border(top: BorderSide(color: AppColors.line)),
      ),
      child: Row(
        children: [
          IconButton(
            tooltip: 'Go to start',
            onPressed: null,
            icon: const Icon(Icons.skip_previous_rounded),
          ),
          IconButton(
            tooltip: 'Play',
            onPressed: null,
            icon: const Icon(
              Icons.play_arrow_rounded,
              color: AppColors.seaGlass,
              size: 25,
            ),
          ),
          IconButton(
            tooltip: 'Stop',
            onPressed: null,
            icon: const Icon(Icons.stop_rounded),
          ),
          const SizedBox(width: 13),
          const Text('--:--', style: _timeStyle),
          if (!compact) ...[
            const SizedBox(width: 16),
            const Expanded(child: _IdlePlayhead()),
            const SizedBox(width: 16),
            Icon(
              Icons.volume_up_outlined,
              color: AppColors.muted.withValues(alpha: 0.8),
              size: 18,
            ),
            const SizedBox(width: 10),
            const SizedBox(
              width: 120,
              child: Slider(value: 0.8, onChanged: null),
            ),
          ] else
            const Spacer(),
          const SizedBox(width: 14),
          Icon(
            Icons.circle,
            color:
                hasAudio
                    ? AppColors.amber
                    : AppColors.muted.withValues(alpha: 0.45),
            size: 7,
          ),
          const SizedBox(width: 7),
          Text(
            hasAudio ? 'Selected' : 'No audio',
            style: Theme.of(context).textTheme.labelMedium,
          ),
        ],
      ),
    );
  }

  static const _timeStyle = TextStyle(
    color: AppColors.text,
    fontFeatures: [FontFeature.tabularFigures()],
    fontSize: 12,
  );
}

class _IdlePlayhead extends StatelessWidget {
  const _IdlePlayhead();

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 26,
      child: Stack(
        alignment: Alignment.centerLeft,
        children: [
          Container(
            height: 3,
            decoration: BoxDecoration(
              color: AppColors.line,
              borderRadius: BorderRadius.circular(3),
            ),
          ),
          Container(
            width: 7,
            height: 7,
            decoration: const BoxDecoration(
              color: AppColors.amber,
              shape: BoxShape.circle,
            ),
          ),
        ],
      ),
    );
  }
}
