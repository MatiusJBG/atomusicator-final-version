import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/di/workbench_providers.dart';
import '../../../core/di/studio_controller.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/theme/app_theme.dart';
import 'studio_panels.dart';

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
    if (selection == null || !mounted) return;
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

  void _addAudio(List<SelectedAudioFile> files) {
    const extensions = {'wav', 'mp3', 'flac', 'm4a'};
    final accepted =
        files
            .where(
              (file) =>
                  extensions.contains(file.name.split('.').last.toLowerCase()),
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
    final sources = ref.watch(workbenchProvider).sources;
    final studio = ref.watch(studioProvider);
    final status = ref.watch(nativeEngineStatusProvider);
    return LayoutBuilder(
      builder: (context, box) {
        final compact = box.maxWidth < 1100;
        return Scaffold(
          body: SafeArea(
            child: Row(
              children: [
                _ToolRail(compact: compact),
                const VerticalDivider(width: 1),
                Expanded(
                  child: Column(
                    children: [
                      _SessionHeader(onImport: _importAudio, compact: compact),
                      Expanded(
                        child: Padding(
                          padding: EdgeInsets.fromLTRB(
                            compact ? 18 : 24,
                            0,
                            compact ? 18 : 24,
                            0,
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              const StudioFeedback(),
                              if (studio.session == null) ...[
                                const SizedBox(height: 8),
                                const _StemStrip(),
                                const SizedBox(height: 12),
                              ],
                              Expanded(
                                child: DropTarget(
                                  onDragEntered:
                                      (_) => setState(() => _dragging = true),
                                  onDragExited:
                                      (_) => setState(() => _dragging = false),
                                  onDragDone: (details) {
                                    setState(() => _dragging = false);
                                    _addAudio([
                                      for (final file in details.files)
                                        if (file.path.isNotEmpty)
                                          SelectedAudioFile(
                                            path: file.path,
                                            name:
                                                file.path
                                                    .split(RegExp(r'[/\\]'))
                                                    .last,
                                            sizeBytes: 0,
                                          ),
                                    ]);
                                  },
                                  child:
                                      studio.session != null
                                          ? const StudioTracks()
                                          : _TrackCanvas(
                                            sources: sources,
                                            onImport: _importAudio,
                                            onRemove:
                                                ref
                                                    .read(
                                                      workbenchProvider
                                                          .notifier,
                                                    )
                                                    .removeSource,
                                            dragging: _dragging,
                                          ),
                                ),
                              ),
                              SizedBox(
                                height: 32,
                                child: Row(
                                  children: [
                                    const Icon(
                                      Icons.headphones_outlined,
                                      size: 14,
                                    ),
                                    const SizedBox(width: 8),
                                    const Expanded(
                                      child: Text(
                                        'M · Mute    S · Solo    Pan · L/R',
                                        style: TextStyle(
                                          fontSize: 10,
                                          color: AppColors.muted,
                                        ),
                                      ),
                                    ),
                                    Text(
                                      studio.session == null
                                          ? 'Original audio preserved'
                                          : '${studio.session!.tracks.length} tracks  /  ${formatStudioTime(studio.duration)}',
                                      style: const TextStyle(
                                        fontSize: 10,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                      const StudioTransport(),
                      Container(
                        height: 24,
                        decoration: const BoxDecoration(
                          border: Border(
                            top: BorderSide(color: AppColors.line),
                          ),
                        ),
                        padding: const EdgeInsets.symmetric(horizontal: 24),
                        child: Row(
                          children: [
                            Expanded(
                              child: Text(
                                ref
                                        .watch(workbenchProvider)
                                        .selectedSource
                                        ?.name ??
                                    studio.session?.name ??
                                    'No source selected',
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontSize: 10,
                                  color: AppColors.muted,
                                ),
                              ),
                            ),
                            const SizedBox(width: 12),
                            Icon(
                              Icons.circle,
                              size: 5,
                              color:
                                  status.hasValue
                                      ? AppColors.seaGlass
                                      : AppColors.amber,
                            ),
                            const SizedBox(width: 6),
                            Text(
                              status.hasValue
                                  ? 'Audio engine ready · Local processing'
                                  : status.isLoading
                                  ? 'Connecting audio engine…'
                                  : 'Audio engine unavailable',
                              style: const TextStyle(
                                fontSize: 10,
                                color: AppColors.muted,
                              ),
                            ),
                          ],
                        ),
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

class _SessionHeader extends ConsumerWidget {
  const _SessionHeader({required this.onImport, required this.compact});
  final VoidCallback onImport;
  final bool compact;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final studio = ref.watch(studioProvider);
    final workbench = ref.watch(workbenchProvider);
    final selected = workbench.selectedSource;
    final available = ref.watch(nativeEngineStatusProvider).hasValue;
    return Padding(
      padding: EdgeInsets.fromLTRB(
        compact ? 18 : 24,
        24,
        compact ? 18 : 24,
        20,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'Atomusicator  /  Audio studio',
                  style: TextStyle(fontSize: 11, color: AppColors.muted),
                ),
                const SizedBox(height: 5),
                Text(
                  studio.session?.name ?? selected?.name ?? 'New session',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.headlineMedium?.copyWith(
                    fontSize: compact ? 21 : 24,
                  ),
                ),
                if (workbench.sources.length > 1)
                  SizedBox(
                    width: 260,
                    height: 26,
                    child: DropdownButton<String>(
                      value: selected?.path,
                      isDense: true,
                      isExpanded: true,
                      items: [
                        for (final source in workbench.sources)
                          DropdownMenuItem(
                            value: source.path,
                            child: Text(
                              source.name,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                      ],
                      onChanged:
                          studio.busy
                              ? null
                              : (path) {
                                if (path != null) {
                                  ref
                                      .read(workbenchProvider.notifier)
                                      .selectSource(path);
                                }
                              },
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          OutlinedButton.icon(
            onPressed: onImport,
            icon: const Icon(Icons.file_upload_outlined, size: 17),
            label: const Text('Import audio'),
          ),
          const SizedBox(width: 12),
          FilledButton.icon(
            onPressed:
                selected != null && available && !studio.busy
                    ? () => ref
                        .read(studioProvider.notifier)
                        .separate(selected.path)
                    : null,
            icon: const Icon(Icons.auto_awesome_outlined, size: 17),
            label: const Text('Separate stems'),
          ),
        ],
      ),
    );
  }
}

class _ToolRail extends ConsumerWidget {
  const _ToolRail({required this.compact});
  final bool compact;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final available = ref.watch(nativeEngineStatusProvider).hasValue;
    return SizedBox(
      width: compact ? 66 : 99,
      child: Column(
        children: [
          const SizedBox(height: 24),
          Container(
            width: 42,
            height: 42,
            decoration: BoxDecoration(
              color: AppColors.seaGlass.withValues(alpha: .18),
              borderRadius: BorderRadius.circular(12),
            ),
            child: const Icon(
              Icons.graphic_eq,
              color: AppColors.seaGlass,
              size: 25,
            ),
          ),
          const SizedBox(height: 30),
          _RailButton(
            icon: Icons.folder_open_outlined,
            label: 'Projects',
            compact: compact,
            onTap:
                available && !state.busy
                    ? () => openStudioSession(context, ref)
                    : null,
          ),
          _RailButton(
            icon: Icons.tune,
            label: 'DAW mixer',
            compact: compact,
            selected: true,
          ),
          _RailButton(
            icon: Icons.file_download_outlined,
            label: 'Export stems',
            compact: compact,
            onTap:
                state.session != null && !state.busy
                    ? () => showDialog<void>(
                      context: context,
                      builder: (_) => const ExportDialog(),
                    )
                    : null,
          ),
          _RailButton(
            icon: Icons.settings_outlined,
            label: 'Projects folder',
            compact: compact,
            onTap:
                available && !state.busy
                    ? () => chooseProjectsDirectory(context, ref)
                    : null,
          ),
          const Spacer(),
          const Icon(Icons.circle, size: 5, color: AppColors.seaGlass),
          const SizedBox(height: 8),
          if (!compact)
            const Text(
              'Local audio',
              style: TextStyle(fontSize: 10, color: AppColors.muted),
            ),
          const SizedBox(height: 20),
        ],
      ),
    );
  }
}

class _RailButton extends StatelessWidget {
  const _RailButton({
    required this.icon,
    required this.label,
    required this.compact,
    this.selected = false,
    this.onTap,
  });
  final IconData icon;
  final String label;
  final bool compact, selected;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
    child: Tooltip(
      message: label,
      child: Material(
        color:
            selected
                ? AppColors.seaGlass.withValues(alpha: .12)
                : Colors.transparent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(
            color:
                selected
                    ? AppColors.seaGlass.withValues(alpha: .3)
                    : Colors.transparent,
          ),
        ),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(8),
          child: SizedBox(
            height: 70,
            width: double.infinity,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  icon,
                  color:
                      selected
                          ? AppColors.seaGlass
                          : onTap == null
                          ? AppColors.line
                          : AppColors.muted,
                ),
                if (!compact) ...[
                  const SizedBox(height: 7),
                  Text(
                    label,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 10,
                      color: selected ? AppColors.seaGlass : AppColors.muted,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

class _StemStrip extends StatelessWidget {
  const _StemStrip();
  @override
  Widget build(BuildContext context) => SizedBox(
    height: 70,
    child: ListView.separated(
      scrollDirection: Axis.horizontal,
      itemCount: stemNames.length,
      separatorBuilder: (_, __) => const SizedBox(width: 8),
      itemBuilder: (_, index) {
        final key = stemNames.keys.elementAt(index);
        return Container(
          width: 150,
          padding: const EdgeInsets.all(9),
          decoration: BoxDecoration(
            color: AppColors.panel,
            border: Border(left: BorderSide(color: stemColor(key), width: 3)),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                stemNames[key]!,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 12),
              ),
              const Text(
                'Not separated',
                style: TextStyle(fontSize: 10, color: AppColors.muted),
              ),
            ],
          ),
        );
      },
    ),
  );
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
              child: SingleChildScrollView(
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
