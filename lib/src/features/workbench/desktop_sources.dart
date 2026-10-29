part of 'audio_workbench.dart';

class _AtomizerScreen extends ConsumerWidget {
  const _AtomizerScreen({required this.onImport, required this.dragging});
  final VoidCallback onImport;
  final bool dragging;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final source = ref.watch(workbenchProvider).selectedSource;
    return _PageViewport(
      minHeight:
          source == null
              ? 720
              : MediaQuery.sizeOf(context).width < 1150
              ? 860
              : 690,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const StudioFeedback(),
          Expanded(
            child:
                source == null
                    ? _EmptyAtomizer(onImport: onImport, dragging: dragging)
                    : _SourceReview(source: source, onImport: onImport),
          ),
        ],
      ),
    );
  }
}

class _PageViewport extends StatelessWidget {
  const _PageViewport({required this.child, this.minHeight = 680});
  final Widget child;
  final double minHeight;
  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final minimum = minHeight * MediaQuery.textScalerOf(context).scale(1);
      return SingleChildScrollView(
        child: SizedBox(
          height: box.maxHeight < minimum ? minimum : box.maxHeight,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(20, 26, 24, 24),
            child: child,
          ),
        ),
      );
    },
  );
}

class _EmptyAtomizer extends StatelessWidget {
  const _EmptyAtomizer({required this.onImport, required this.dragging});
  final VoidCallback onImport;
  final bool dragging;
  @override
  Widget build(BuildContext context) => Align(
    alignment: Alignment.topLeft,
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 1248),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionLabel('PASO 1 DE 3 · IMPORTA TU AUDIO'),
          const SizedBox(height: 8),
          const Text(
            'Nada se ha cargado todavía. Importa un archivo para iniciar la separación.',
            style: TextStyle(fontSize: 10, color: AppColors.muted),
          ),
          const SizedBox(height: 10),
          Expanded(
            child: CustomPaint(
              foregroundPainter: _DropBorderPainter(dragging),
              child: Container(
                decoration: BoxDecoration(
                  color: const Color(0xFF0E141B),
                  borderRadius: BorderRadius.circular(18),
                ),
                child: _DropInvitation(onImport: onImport, dragging: dragging),
              ),
            ),
          ),
          const SizedBox(height: 26),
          const SizedBox(
            height: 196,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: _InstructionCard(
                    number: '1',
                    title: 'Importa tu mezcla',
                    description:
                        'Arrastra un WAV, MP3, FLAC o M4A. Se analiza el audio y se muestra la forma de onda completa.',
                  ),
                ),
                SizedBox(width: 24),
                Expanded(
                  child: _InstructionCard(
                    number: '2',
                    title: 'Separa los stems',
                    description:
                        'La IA divide la mezcla en voz, coros, batería, bajo e instrumental, con progreso por etapas.',
                  ),
                ),
                SizedBox(width: 24),
                Expanded(
                  child: _InstructionCard(
                    number: '3',
                    title: 'Mezcla y exporta',
                    description:
                        'Ajusta volumen, paneo y pitch en el mezclador y exporta cada stem por separado.',
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 20),
          Row(
            children: [
              const Icon(
                Icons.shield_outlined,
                color: AppColors.seaGlass,
                size: 18,
              ),
              const SizedBox(width: 10),
              const Expanded(
                child: Text(
                  'Todo se procesa en tu equipo: el audio nunca se sube a la nube.',
                  style: TextStyle(fontSize: 10, color: AppColors.muted),
                ),
              ),
              TextButton(
                onPressed: () => _showStudioHelp(context),
                child: const Text(
                  'Formatos y límites',
                  style: TextStyle(fontSize: 10),
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
        ],
      ),
    ),
  );
}

class _DropInvitation extends StatelessWidget {
  const _DropInvitation({required this.onImport, required this.dragging});
  final VoidCallback onImport;
  final bool dragging;
  @override
  Widget build(BuildContext context) => Center(
    child: SingleChildScrollView(
      padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 76,
            height: 76,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: AppColors.seaGlass.withValues(alpha: .05),
            ),
            child: Center(
              child: Container(
                width: 56,
                height: 56,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: AppColors.seaGlass.withValues(alpha: .08),
                  border: Border.all(
                    color: AppColors.seaGlass.withValues(alpha: .15),
                  ),
                ),
                child: const Icon(
                  Icons.auto_awesome,
                  color: AppColors.seaGlass,
                  size: 25,
                ),
              ),
            ),
          ),
          const SizedBox(height: 14),
          Text(
            dragging ? 'Suelta tu audio aquí' : 'Arrastra tu audio aquí',
            textAlign: TextAlign.center,
            style: const TextStyle(
              fontSize: 26,
              fontWeight: FontWeight.w700,
              letterSpacing: -.7,
            ),
          ),
          const SizedBox(height: 8),
          const Text(
            'o haz clic en Importar audio para iniciar',
            textAlign: TextAlign.center,
            style: TextStyle(fontSize: 12, color: AppColors.muted),
          ),
          const SizedBox(height: 14),
          const Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              _Tag('WAV'),
              SizedBox(width: 10),
              _Tag('FLAC'),
              SizedBox(width: 10),
              _Tag('MP3'),
              SizedBox(width: 10),
              _Tag('M4A'),
            ],
          ),
          const SizedBox(height: 16),
          SizedBox(
            width: 220,
            child: _CyanButton(
              label: 'Importar audio',
              height: 54,
              icon: Icons.file_upload_outlined,
              onPressed: onImport,
            ),
          ),
          const SizedBox(height: 12),
          const Text(
            'El audio se procesa en tu equipo · Ctrl+V para pegar archivos',
            textAlign: TextAlign.center,
            style: TextStyle(fontSize: 9, color: AppColors.muted),
          ),
        ],
      ),
    ),
  );
}

class _InstructionCard extends StatelessWidget {
  const _InstructionCard({
    required this.number,
    required this.title,
    required this.description,
  });
  final String number, title, description;
  @override
  Widget build(BuildContext context) => _Panel(
    padding: const EdgeInsets.all(22),
    child: SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          CircleAvatar(
            radius: 17,
            backgroundColor: AppColors.seaGlass.withValues(alpha: .09),
            child: Text(
              number,
              style: const TextStyle(
                color: Color(0xFF7FE9D9),
                fontSize: 12,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
          const SizedBox(height: 22),
          Text(
            title,
            style: const TextStyle(fontSize: 12.4, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 10),
          Text(
            description,
            style: const TextStyle(
              fontSize: 9.5,
              height: 1.6,
              color: AppColors.muted,
            ),
          ),
        ],
      ),
    ),
  );
}

class _DropBorderPainter extends CustomPainter {
  _DropBorderPainter(this.active);
  final bool active;
  @override
  void paint(Canvas canvas, Size size) {
    final path =
        Path()..addRRect(
          RRect.fromRectAndRadius(
            (Offset.zero & size).deflate(1),
            const Radius.circular(18),
          ),
        );
    final paint =
        Paint()
          ..color = active ? AppColors.seaGlass : const Color(0xFF2A3A46)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1;
    for (final metric in path.computeMetrics()) {
      for (var distance = 0.0; distance < metric.length; distance += 16) {
        canvas.drawPath(metric.extractPath(distance, distance + 7), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_DropBorderPainter old) => old.active != active;
}

class _SourceReview extends ConsumerWidget {
  const _SourceReview({required this.source, required this.onImport});
  final SelectedAudioFile source;
  final VoidCallback onImport;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(workbenchProvider);
    final selected = ref.watch(selectedStemTypesProvider);
    final preview = ref.watch(sourceAudioPreviewProvider(source.path));
    final audio = preview.valueOrNull;
    final available = ref.watch(nativeEngineStatusProvider).hasValue;
    final busy = ref.watch(studioProvider.select((s) => s.busy));
    return Column(
      children: [
        if (state.sources.length > 1)
          SizedBox(
            height: 46,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              itemCount: state.sources.length,
              separatorBuilder: (_, __) => const SizedBox(width: 8),
              itemBuilder: (_, i) {
                final file = state.sources[i];
                return InputChip(
                  key: ValueKey('source-${file.path}'),
                  selected: file.path == source.path,
                  label: Text(file.name, style: const TextStyle(fontSize: 10)),
                  onPressed:
                      busy
                          ? null
                          : () => ref
                              .read(workbenchProvider.notifier)
                              .selectSource(file.path),
                  onDeleted:
                      busy
                          ? null
                          : () => ref
                              .read(workbenchProvider.notifier)
                              .removeSource(file.path),
                );
              },
            ),
          ),
        SizedBox(
          height: 238,
          child: _Panel(
            padding: const EdgeInsets.all(18),
            child: Column(
              children: [
                Row(
                  children: [
                    const Icon(
                      Icons.graphic_eq,
                      color: AppColors.seaGlass,
                      size: 24,
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            source.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 5),
                          Text(
                            _fileSize(source.sizeBytes),
                            style: const TextStyle(
                              fontSize: 9,
                              color: AppColors.muted,
                            ),
                          ),
                        ],
                      ),
                    ),
                    if (audio != null) ...[
                      _Tag(formatStudioTime(audio.duration)),
                      const SizedBox(width: 8),
                      _Tag(_audioMeta(audio.sampleRate, audio.channels)),
                      if (audio.bitDepth != null) ...[
                        const SizedBox(width: 8),
                        _Tag('${audio.bitDepth}-bit'),
                      ],
                    ],
                  ],
                ),
                const SizedBox(height: 16),
                Expanded(
                  child: SizedBox(
                    width: double.infinity,
                    child: preview.when(
                      data:
                          (value) =>
                              value == null
                                  ? const Center(
                                    child: Text(
                                      'La vista previa se mostrará al leer el archivo.',
                                      style: TextStyle(
                                        fontSize: 11,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                  )
                                  : CustomPaint(
                                    painter: _AudioPreviewPainter(
                                      value.peaks,
                                      AppColors.muted,
                                    ),
                                  ),
                      loading:
                          () => const Center(
                            child: SizedBox(
                              width: 22,
                              height: 22,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            ),
                          ),
                      error:
                          (_, __) => const Center(
                            child: Text(
                              'No se pudo generar la vista previa. Puedes cambiar el audio.',
                              style: TextStyle(
                                fontSize: 11,
                                color: AppColors.muted,
                              ),
                            ),
                          ),
                    ),
                  ),
                ),
                const SizedBox(height: 10),
                Row(
                  children: [
                    const Text(
                      '0:00',
                      style: TextStyle(fontSize: 9, color: AppColors.muted),
                    ),
                    const Expanded(
                      child: Text(
                        'Revisa la mezcla original antes de separar',
                        textAlign: TextAlign.center,
                        style: TextStyle(fontSize: 9, color: AppColors.muted),
                      ),
                    ),
                    Text(
                      audio == null ? '—:—' : formatStudioTime(audio.duration),
                      style: const TextStyle(
                        fontSize: 9,
                        color: AppColors.muted,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 20),
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                flex: 6,
                child: _Panel(
                  padding: const EdgeInsets.all(18),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          const Expanded(
                            child: _SectionLabel('STEMS A GENERAR'),
                          ),
                          TextButton(
                            onPressed:
                                () =>
                                    ref
                                        .read(
                                          selectedStemTypesProvider.notifier,
                                        )
                                        .state = stemNames.keys.toSet(),
                            child: const Text(
                              'Todos',
                              style: TextStyle(fontSize: 10),
                            ),
                          ),
                        ],
                      ),
                      const Text(
                        '5 pistas independientes para tu mezcla',
                        style: TextStyle(fontSize: 10, color: AppColors.muted),
                      ),
                      const SizedBox(height: 12),
                      Expanded(
                        child: Column(
                          children: [
                            for (final kind in stemNames.keys)
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.only(bottom: 6),
                                  child: _Panel(
                                    color: const Color(0xFF0F181F),
                                    radius: 8,
                                    padding: EdgeInsets.zero,
                                    child: CheckboxListTile(
                                      value: selected.contains(kind),
                                      dense: true,
                                      controlAffinity:
                                          ListTileControlAffinity.leading,
                                      contentPadding:
                                          const EdgeInsets.symmetric(
                                            horizontal: 8,
                                          ),
                                      onChanged:
                                          busy
                                              ? null
                                              : (checked) {
                                                ref
                                                    .read(
                                                      selectedStemTypesProvider
                                                          .notifier,
                                                    )
                                                    .state = checked == true
                                                        ? {...selected, kind}
                                                        : ({...selected}
                                                          ..remove(kind));
                                              },
                                      title: Row(
                                        children: [
                                          Icon(
                                            stemIcon(kind),
                                            color: stemColor(kind),
                                            size: 20,
                                          ),
                                          const SizedBox(width: 10),
                                          Text(
                                            _stemLabel(kind),
                                            style: const TextStyle(
                                              fontSize: 11,
                                              fontWeight: FontWeight.w600,
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 8),
                      const Text(
                        'Las pistas elegidas se incluyen al exportar. El motor procesa los cinco stems.',
                        style: TextStyle(fontSize: 9, color: AppColors.muted),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 20),
              Expanded(
                flex: 5,
                child: _Panel(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const _SectionLabel('RESUMEN'),
                      const SizedBox(height: 18),
                      const _SummaryLine('Modelo', 'UVR / Kuielab · MDX'),
                      const _SummaryLine(
                        'Dispositivo',
                        'CPU · procesamiento local',
                      ),
                      _SummaryLine(
                        'Calidad',
                        audio == null
                            ? 'Por analizar'
                            : _audioMeta(audio.sampleRate, audio.channels),
                      ),
                      const _SummaryLine(
                        'Duración estimada',
                        'Depende del audio y tu equipo',
                      ),
                      const SizedBox(height: 14),
                      const Text(
                        'La separación corre en segundo plano: puedes seguir usando la app.',
                        style: TextStyle(
                          fontSize: 10,
                          height: 1.6,
                          color: AppColors.muted,
                        ),
                      ),
                      const Spacer(),
                      _CyanButton(
                        label: 'Separar ${selected.length} stems',
                        height: 54,
                        onPressed:
                            available && !busy && selected.isNotEmpty
                                ? () {
                                  ref
                                      .read(separationFlowProvider.notifier)
                                      .state = true;
                                  ref
                                      .read(separationResultProvider.notifier)
                                      .state = false;
                                  ref
                                      .read(studioProvider.notifier)
                                      .separate(source.path);
                                }
                                : null,
                      ),
                      const SizedBox(height: 10),
                      TextButton(
                        onPressed:
                            () =>
                                ref.read(studioViewProvider.notifier).state =
                                    StudioView.settings,
                        child: const Text(
                          'Modelo y ajustes avanzados',
                          style: TextStyle(fontSize: 10),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _SummaryLine extends StatelessWidget {
  const _SummaryLine(this.label, this.value);
  final String label, value;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 15),
    child: Row(
      children: [
        Text(
          label,
          style: const TextStyle(fontSize: 10, color: AppColors.muted),
        ),
        const SizedBox(width: 16),
        Expanded(
          child: Text(
            value,
            textAlign: TextAlign.right,
            style: const TextStyle(fontSize: 10, fontWeight: FontWeight.w600),
          ),
        ),
      ],
    ),
  );
}

class _AudioPreviewPainter extends CustomPainter {
  _AudioPreviewPainter(this.peaks, this.color, {this.progress});
  final List<double> peaks;
  final Color color;
  final double? progress;
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    final count = peaks.length ~/ 2;
    final path = Path();
    for (var i = 0; i < count; i++) {
      final x = i * size.width / (count > 1 ? count - 1 : 1);
      final y = size.height * (.5 - peaks[i * 2 + 1].clamp(-1, 1) * .43);
      if (i == 0) {
        path.moveTo(x, y);
      } else {
        path.lineTo(x, y);
      }
    }
    for (var i = count - 1; i >= 0; i--) {
      path.lineTo(
        i * size.width / (count > 1 ? count - 1 : 1),
        size.height * (.5 - peaks[i * 2].clamp(-1, 1) * .43),
      );
    }
    if (count > 0) {
      path.close();
      canvas.drawPath(path, paint);
    }
    if (progress != null) {
      canvas.drawLine(
        Offset(size.width * progress!, 0),
        Offset(size.width * progress!, size.height),
        Paint()
          ..color = AppColors.playhead
          ..strokeWidth = 1.5,
      );
    }
  }

  @override
  bool shouldRepaint(_AudioPreviewPainter old) =>
      old.peaks != peaks || old.color != color || old.progress != progress;
}
