part of 'studio_panels.dart';

class ExportDialog extends ConsumerStatefulWidget {
  const ExportDialog({super.key});
  @override
  ConsumerState<ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends ConsumerState<ExportDialog> {
  String format = 'wav';
  int rate = 44100, channels = 2, bits = 24;
  bool normalize = false, includeMuted = true;
  String? destination;
  final selected = <String>{};
  @override
  void initState() {
    super.initState();
    selected.addAll(
      ref.read(studioProvider).session!.tracks.map((t) => t.stemType),
    );
  }

  Future<void> _chooseFolder() async {
    final folder = await FilePicker.platform.getDirectoryPath(
      dialogTitle: 'Export destination',
    );
    if (folder != null && mounted) setState(() => destination = folder);
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(studioProvider);
    final tracks = state.session!.tracks;
    final exported =
        tracks
            .where(
              (t) =>
                  selected.contains(t.stemType) &&
                  (includeMuted || !(state.tracks[t.stemType]?.mute ?? false)),
            )
            .map((t) => t.stemType)
            .toList();
    return AlertDialog(
      insetPadding: const EdgeInsets.all(24),
      contentPadding: const EdgeInsets.fromLTRB(28, 8, 28, 20),
      titlePadding: const EdgeInsets.fromLTRB(28, 20, 20, 0),
      title: Row(
        children: [
          const Expanded(child: Text('Export stems')),
          IconButton(
            tooltip: 'Close',
            onPressed: () => Navigator.pop(context),
            icon: const Icon(Icons.close, size: 18),
          ),
        ],
      ),
      content: SizedBox(
        width: 568,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '${state.session!.name} · ${tracks.length} separate tracks, ready for your DAW.',
                style: const TextStyle(fontSize: 12, color: AppColors.muted),
              ),
              const SizedBox(height: 22),
              const _FieldLabel('Audio format'),
              Row(
                children: [
                  for (final value in ['wav', 'flac', 'mp3']) ...[
                    if (value != 'wav') const SizedBox(width: 10),
                    Expanded(
                      child: _FormatOption(
                        value: value,
                        selected: format == value,
                        onTap: () => setState(() => format = value),
                      ),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 18),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: _ExportSelect<int>(
                      label: 'Sample rate',
                      value: rate,
                      options: const {44100: '44.1 kHz', 48000: '48 kHz'},
                      onChanged: (v) => setState(() => rate = v),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: _ExportSelect<int>(
                      label: format == 'mp3' ? 'Bit rate' : 'Bit depth',
                      value: format == 'mp3' ? 320 : bits,
                      options:
                          format == 'mp3'
                              ? const {320: '320 kbps'}
                              : const {16: '16 bits', 24: '24 bits'},
                      onChanged:
                          format == 'mp3'
                              ? null
                              : (v) => setState(() => bits = v),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: _ExportSelect<int>(
                      label: 'Channels',
                      value: channels,
                      options: const {1: 'Mono', 2: 'Stereo'},
                      onChanged: (v) => setState(() => channels = v),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 20),
              Row(
                children: [
                  const Expanded(child: _FieldLabel('Stems to export')),
                  Text(
                    '${exported.length} OF ${tracks.length} SELECTED',
                    style: const TextStyle(
                      fontSize: 9,
                      color: AppColors.seaGlass,
                    ),
                  ),
                ],
              ),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final track in tracks)
                    FilterChip(
                      key: ValueKey('export-${track.stemType}'),
                      avatar: Icon(
                        Icons.circle,
                        size: 6,
                        color: stemColor(track.stemType),
                      ),
                      label: Text(
                        stemNames[track.stemType] ?? track.stemType,
                        style: const TextStyle(
                          fontSize: 10,
                          color: AppColors.text,
                        ),
                      ),
                      selected: selected.contains(track.stemType),
                      showCheckmark: false,
                      selectedColor: AppColors.panelRaised,
                      backgroundColor: AppColors.canvas,
                      side: BorderSide(
                        color:
                            selected.contains(track.stemType)
                                ? AppColors.line
                                : Colors.transparent,
                      ),
                      onSelected:
                          (v) => setState(() {
                            if (v) {
                              selected.add(track.stemType);
                            } else {
                              selected.remove(track.stemType);
                            }
                          }),
                    ),
                ],
              ),
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                title: const Text(
                  'Include muted tracks',
                  style: TextStyle(fontSize: 12),
                ),
                value: includeMuted,
                onChanged: (v) => setState(() => includeMuted = v ?? true),
              ),
              const SizedBox(height: 12),
              const _FieldLabel('Save to'),
              Container(
                padding: const EdgeInsets.only(left: 12),
                decoration: BoxDecoration(
                  color: AppColors.canvas,
                  border: Border.all(color: AppColors.line),
                  borderRadius: BorderRadius.circular(5),
                ),
                child: Row(
                  children: [
                    const Icon(Icons.folder_open_outlined, size: 18),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        destination ?? 'Choose a local folder',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontSize: 11),
                      ),
                    ),
                    TextButton(
                      onPressed: _chooseFolder,
                      child: const Text('Change'),
                    ),
                  ],
                ),
              ),
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                title: const Text(
                  'Normalize loudness',
                  style: TextStyle(fontSize: 12),
                ),
                value: normalize,
                onChanged: (v) => setState(() => normalize = v ?? false),
              ),
              const Text(
                'Applied pitch is included. Listening gain, pan and master do not affect export.',
                style: TextStyle(fontSize: 11, color: AppColors.muted),
              ),
              const Divider(height: 30),
              Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '${exported.length} files · ${format.toUpperCase()}',
                          style: const TextStyle(fontSize: 11),
                        ),
                        const SizedBox(height: 4),
                        Text(
                          'Full duration · 00:00–${formatStudioTime(state.duration)}',
                          style: const TextStyle(
                            fontSize: 10,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                  TextButton(
                    onPressed: () => Navigator.pop(context),
                    child: const Text('Cancel'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton.icon(
                    key: const ValueKey('confirm-export'),
                    onPressed:
                        exported.isEmpty || state.busy
                            ? null
                            : () async {
                              if (destination == null) await _chooseFolder();
                              if (destination != null && context.mounted) {
                                ref
                                    .read(studioProvider.notifier)
                                    .export(
                                      destination!,
                                      exported,
                                      format,
                                      rate,
                                      channels,
                                      bits,
                                      normalize,
                                    );
                                Navigator.pop(context);
                              }
                            },
                    icon: const Icon(Icons.file_download_outlined, size: 17),
                    label: const Text('Export stems'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _FieldLabel extends StatelessWidget {
  const _FieldLabel(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Text(
      text,
      style: const TextStyle(fontSize: 11, color: AppColors.muted),
    ),
  );
}

class _FormatOption extends StatelessWidget {
  const _FormatOption({
    required this.value,
    required this.selected,
    required this.onTap,
  });
  final String value;
  final bool selected;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    selected: selected,
    child: Material(
      color:
          selected
              ? AppColors.seaGlass.withValues(alpha: .13)
              : AppColors.canvas,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(5),
        side: BorderSide(color: selected ? AppColors.seaGlass : AppColors.line),
      ),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(5),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      value.toUpperCase(),
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: selected ? AppColors.seaGlass : AppColors.text,
                      ),
                    ),
                  ),
                  Icon(
                    selected
                        ? Icons.radio_button_checked
                        : Icons.radio_button_unchecked,
                    size: 13,
                    color: selected ? AppColors.seaGlass : AppColors.muted,
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Text(switch (value) {
                'wav' => 'Uncompressed',
                'flac' => 'Lossless',
                _ => 'Compressed',
              }, style: const TextStyle(fontSize: 10, color: AppColors.muted)),
            ],
          ),
        ),
      ),
    ),
  );
}

class _ExportSelect<T> extends StatelessWidget {
  const _ExportSelect({
    required this.label,
    required this.value,
    required this.options,
    this.onChanged,
  });
  final String label;
  final T value;
  final Map<T, String> options;
  final ValueChanged<T>? onChanged;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _FieldLabel(label),
      DropdownButtonFormField<T>(
        key: ValueKey('$label-$value'),
        initialValue: value,
        isExpanded: true,
        style: const TextStyle(
          fontFamily: 'monospace',
          fontSize: 11,
          color: AppColors.text,
        ),
        decoration: InputDecoration(
          labelText: label,
          floatingLabelBehavior: FloatingLabelBehavior.never,
          contentPadding: const EdgeInsets.symmetric(
            horizontal: 12,
            vertical: 10,
          ),
        ),
        items: [
          for (final option in options.entries)
            DropdownMenuItem(value: option.key, child: Text(option.value)),
        ],
        onChanged:
            onChanged == null
                ? null
                : (v) {
                  if (v != null) onChanged!(v);
                },
      ),
    ],
  );
}
