import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/di/workbench_providers.dart';
import '../../../core/theme/app_theme.dart';

class AudioWorkbench extends ConsumerWidget {
  const AudioWorkbench({super.key});

  Future<void> _importAudio(WidgetRef ref) async {
    final selection = await FilePicker.platform.pickFiles(
      type: FileType.audio,
      allowMultiple: true,
      withData: false,
    );
    if (selection == null) return;
    ref.read(workbenchProvider.notifier).addSources([
      for (final file in selection.files)
        if (file.path != null)
          SelectedAudioFile(
            path: file.path!,
            name: file.name,
            sizeBytes: file.size,
          ),
    ]);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workbench = ref.watch(workbenchProvider);
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
                      _TopBar(onImport: () => _importAudio(ref)),
                      const Divider(height: 1),
                      Expanded(
                        child: _WorkbenchBody(
                          sources: workbench.sources,
                          compact: compact,
                          onImport: () => _importAudio(ref),
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
  });
  final List<SelectedAudioFile> sources;
  final bool compact;
  final VoidCallback onImport;

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
          _SessionHeader(hasSource: sources.isNotEmpty),
          const SizedBox(height: 19),
          const _StemStrip(),
          const SizedBox(height: 14),
          Expanded(child: _TrackCanvas(sources: sources, onImport: onImport)),
        ],
      ),
    );
  }
}

class _SessionHeader extends StatelessWidget {
  const _SessionHeader({required this.hasSource});
  final bool hasSource;

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
        OutlinedButton.icon(
          onPressed: null,
          icon: const Icon(Icons.auto_awesome_outlined, size: 16),
          label: const Text('Separate stems'),
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
  const _TrackCanvas({required this.sources, required this.onImport});
  final List<SelectedAudioFile> sources;
  final VoidCallback onImport;

  @override
  Widget build(BuildContext context) {
    return Container(
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: AppColors.panel.withValues(alpha: 0.52),
        border: Border.all(color: AppColors.line),
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
                      width: 56,
                      height: 56,
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
                    const SizedBox(height: 10),
                    Text(
                      'Your mix starts here',
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      'Bring in a song to build a five-stem session. Your original file stays where it is.',
                      textAlign: TextAlign.center,
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                    const SizedBox(height: 12),
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
              itemBuilder: (context, index) => _SourceRow(file: sources[index]),
            ),
        ],
      ),
    );
  }
}

class _SourceRow extends StatelessWidget {
  const _SourceRow({required this.file});
  final SelectedAudioFile file;

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
        ],
      ),
    );
  }

  String _formatBytes(int bytes) {
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
