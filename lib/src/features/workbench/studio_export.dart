part of 'studio_panels.dart';

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
