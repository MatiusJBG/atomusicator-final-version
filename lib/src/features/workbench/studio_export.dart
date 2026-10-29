part of 'studio_panels.dart';

class ExportDialog extends ConsumerStatefulWidget {
  const ExportDialog({super.key});
  @override
  ConsumerState<ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends ConsumerState<ExportDialog> {
  String format = 'wav';
  int rate = 44100, channels = 2, bits = 24;
  bool normalize = false,
      includeMuted = true,
      exporting = false,
      advanced = false;
  String? destination;
  final selected = <String>{};
  @override
  void initState() {
    super.initState();
    selected.addAll(
      ref
          .read(studioProvider)
          .session!
          .tracks
          .map((t) => t.stemType)
          .where(ref.read(selectedStemTypesProvider).contains),
    );
  }

  Future<void> _chooseFolder() async {
    try {
      final folder = await FilePicker.platform.getDirectoryPath(
        dialogTitle: 'Destino de exportación',
      );
      if (folder != null && mounted) setState(() => destination = folder);
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('No se pudo abrir el selector: $error')),
        );
      }
    }
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
    final exceeds = tracks.any(
      (t) => exported.contains(t.stemType) && t.peaks.any((v) => v.abs() > 1),
    );
    final completed =
        exporting && state.job?.status == native.UiJobStatus.completed;
    return Dialog(
      backgroundColor: const Color(0xFF0E141B),
      insetPadding: const EdgeInsets.all(24),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: const BorderSide(color: Color(0xFF22302F)),
      ),
      child: SizedBox(
        width: 600,
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(22),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  const Icon(
                    Icons.file_download_outlined,
                    color: AppColors.seaGlass,
                    size: 26,
                  ),
                  const SizedBox(width: 12),
                  const Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Exportar stems',
                          style: TextStyle(
                            fontSize: 20,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        SizedBox(height: 5),
                        Text(
                          'Elige las pistas y el formato de salida',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: 'Cerrar',
                    onPressed:
                        exporting && state.busy
                            ? null
                            : () => Navigator.pop(context),
                    icon: const Icon(Icons.close, size: 22),
                  ),
                ],
              ),
              const Divider(height: 26),
              Row(
                children: [
                  const Expanded(child: _FieldLabel('PISTAS')),
                  Text(
                    '${exported.length} de ${tracks.length}',
                    style: const TextStyle(
                      fontSize: 10,
                      color: AppColors.seaGlass,
                    ),
                  ),
                ],
              ),
              Wrap(
                spacing: 12,
                runSpacing: 8,
                children: [
                  for (final track in tracks)
                    SizedBox(
                      width: 272,
                      height: 38,
                      child: Material(
                        color: AppColors.panel,
                        borderRadius: BorderRadius.circular(10),
                        child: InkWell(
                          key: ValueKey('export-${track.stemType}'),
                          borderRadius: BorderRadius.circular(10),
                          onTap:
                              state.busy
                                  ? null
                                  : () => setState(() {
                                    if (!selected.remove(track.stemType)) {
                                      selected.add(track.stemType);
                                    }
                                  }),
                          child: Row(
                            children: [
                              Checkbox(
                                value: selected.contains(track.stemType),
                                onChanged:
                                    state.busy
                                        ? null
                                        : (v) => setState(() {
                                          if (v == true) {
                                            selected.add(track.stemType);
                                          } else {
                                            selected.remove(track.stemType);
                                          }
                                        }),
                              ),
                              Icon(
                                stemIcon(track.stemType),
                                size: 18,
                                color: stemColor(track.stemType),
                              ),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  stemNames[track.stemType] ?? track.stemType,
                                  style: const TextStyle(
                                    fontSize: 11,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                              ),
                              Text(
                                track.channels == 1 ? 'mono' : 'estéreo',
                                style: const TextStyle(
                                  fontSize: 8,
                                  color: AppColors.muted,
                                ),
                              ),
                              const SizedBox(width: 10),
                            ],
                          ),
                        ),
                      ),
                    ),
                  const SizedBox(
                    width: 272,
                    height: 38,
                    child: Tooltip(
                      message:
                          'La exportación actual admite stems individuales. La mezcla master aún no está disponible.',
                      child: Opacity(
                        opacity: .4,
                        child: Row(
                          children: [
                            SizedBox(width: 12),
                            Icon(Icons.check_box_outline_blank, size: 18),
                            SizedBox(width: 12),
                            Icon(
                              Icons.graphic_eq,
                              size: 18,
                              color: AppColors.seaGlass,
                            ),
                            SizedBox(width: 8),
                            Text(
                              'Mezcla Master',
                              style: TextStyle(fontSize: 11),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 18),
              const _FieldLabel('FORMATO'),
              Row(
                children: [
                  for (final value in ['wav', 'flac', 'mp3']) ...[
                    if (value != 'wav') const SizedBox(width: 10),
                    Expanded(
                      child: ChoiceChip(
                        showCheckmark: true,
                        selected: format == value,
                        label: Text(
                          value == 'mp3'
                              ? 'MP3 320 kbps'
                              : '${value.toUpperCase()} ${value == 'flac' && bits == 32 ? 24 : bits}-bit',
                          style: const TextStyle(fontSize: 10),
                        ),
                        onSelected:
                            state.busy
                                ? null
                                : (_) => setState(() {
                                  format = value;
                                  if (format != 'wav' && bits == 32) bits = 24;
                                }),
                      ),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 14),
              const _FieldLabel('DESTINO'),
              Container(
                padding: const EdgeInsets.fromLTRB(12, 0, 6, 0),
                decoration: BoxDecoration(
                  color: AppColors.canvas,
                  border: Border.all(color: AppColors.line),
                  borderRadius: BorderRadius.circular(9),
                ),
                child: Row(
                  children: [
                    const Icon(Icons.folder_open_outlined, size: 19),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        destination ?? 'Elige una carpeta local',
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontSize: 10),
                      ),
                    ),
                    TextButton(
                      onPressed: state.busy ? null : _chooseFolder,
                      child: const Text(
                        'Cambiar',
                        style: TextStyle(fontSize: 10),
                      ),
                    ),
                  ],
                ),
              ),
              TextButton.icon(
                onPressed: () => setState(() => advanced = !advanced),
                icon: Icon(
                  advanced ? Icons.expand_less : Icons.expand_more,
                  size: 16,
                ),
                label: const Text(
                  'Opciones avanzadas',
                  style: TextStyle(fontSize: 10),
                ),
              ),
              if (advanced) ...[
                Row(
                  children: [
                    Expanded(
                      child: _ExportSelect<int>(
                        label: 'Frecuencia',
                        value: rate,
                        options: const {44100: '44.1 kHz', 48000: '48 kHz'},
                        onChanged: (v) => setState(() => rate = v),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: _ExportSelect<int>(
                        label: format == 'mp3' ? 'Bit rate' : 'Profundidad',
                        value: format == 'mp3' ? 320 : bits,
                        options:
                            format == 'mp3'
                                ? const {320: '320 kbps'}
                                : format == 'wav'
                                ? const {
                                  16: '16 bits',
                                  24: '24 bits',
                                  32: '32-bit float',
                                }
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
                        label: 'Canales',
                        value: channels,
                        options: const {1: 'Mono', 2: 'Estéreo'},
                        onChanged: (v) => setState(() => channels = v),
                      ),
                    ),
                  ],
                ),
                CheckboxListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  controlAffinity: ListTileControlAffinity.leading,
                  title: const Text(
                    'Incluir pistas silenciadas',
                    style: TextStyle(fontSize: 11),
                  ),
                  value: includeMuted,
                  onChanged:
                      state.busy
                          ? null
                          : (v) => setState(() => includeMuted = v ?? true),
                ),
                CheckboxListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  controlAffinity: ListTileControlAffinity.leading,
                  title: const Text(
                    'Normalizar volumen',
                    style: TextStyle(fontSize: 11),
                  ),
                  value: normalize,
                  onChanged:
                      state.busy
                          ? null
                          : (v) => setState(() => normalize = v ?? false),
                ),
                const Text(
                  'El tono aplicado se incluye. La ganancia, el paneo y el master de escucha no modifican los archivos exportados.',
                  style: TextStyle(fontSize: 10, color: AppColors.muted),
                ),
                const SizedBox(height: 10),
              ],
              if (exceeds && !normalize && !(format == 'wav' && bits == 32))
                const Padding(
                  padding: EdgeInsets.only(bottom: 10),
                  child: Text(
                    'Las pistas elegidas superan 0 dBFS. Elige WAV de 32-bit float o activa la normalización.',
                    style: TextStyle(fontSize: 10, color: AppColors.amber),
                  ),
                ),
              if (normalize)
                const Padding(
                  padding: EdgeInsets.only(bottom: 10),
                  child: Text(
                    'La normalización se aplica a cada archivo y puede cambiar los niveles relativos de los stems.',
                    style: TextStyle(fontSize: 10, color: AppColors.muted),
                  ),
                ),
              if (exporting) ...[
                Row(
                  children: [
                    const Expanded(child: _FieldLabel('PROGRESO')),
                    Text(
                      completed
                          ? 'Exportación completada'
                          : '${((state.job?.progress ?? 0) * 100).round()} %',
                      style: const TextStyle(
                        fontSize: 10,
                        color: AppColors.seaGlass,
                      ),
                    ),
                  ],
                ),
                LinearProgressIndicator(
                  value: completed ? 1 : state.job?.progress,
                  minHeight: 5,
                ),
                const SizedBox(height: 12),
                if (state.error != null)
                  Text(
                    state.error!,
                    style: const TextStyle(
                      fontSize: 10,
                      color: AppColors.coral,
                    ),
                  ),
              ],
              const SizedBox(height: 8),
              Row(
                children: [
                  Expanded(
                    child: OutlinedButton(
                      onPressed: () {
                        if (exporting && state.busy) {
                          ref.read(studioProvider.notifier).cancel();
                        } else {
                          Navigator.pop(context);
                        }
                      },
                      child: Text(completed ? 'Cerrar' : 'Cancelar'),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 3,
                    child: FilledButton.icon(
                      key: const ValueKey('confirm-export'),
                      onPressed:
                          exported.isEmpty || state.busy || completed
                              ? null
                              : () async {
                                if (destination == null) await _chooseFolder();
                                if (destination != null && mounted) {
                                  setState(() => exporting = true);
                                  await ref
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
                                }
                              },
                      icon: const Icon(Icons.file_download_outlined, size: 20),
                      label: Text(
                        completed ? 'Exportado' : 'Exportar ahora',
                        style: const TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
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
    padding: const EdgeInsets.only(bottom: 7),
    child: Text(
      text,
      style: const TextStyle(
        fontSize: 9,
        letterSpacing: 1,
        fontWeight: FontWeight.w700,
        color: AppColors.muted,
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
          fontFamily: 'Inter',
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
