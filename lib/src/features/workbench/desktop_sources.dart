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
              ? 1020
              : 850,
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
      final phone = isPhoneLayout(context) || isCompactLandscape(context);
      return SingleChildScrollView(
        child: SizedBox(
          height: box.maxHeight < minimum ? minimum : box.maxHeight,
          child: Padding(
            padding:
                phone
                    ? const EdgeInsets.fromLTRB(14, 10, 14, 14)
                    : const EdgeInsets.fromLTRB(20, 26, 24, 24),
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

class _DropInvitation extends ConsumerWidget {
  const _DropInvitation({required this.onImport, required this.dragging});
  final VoidCallback onImport;
  final bool dragging;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final importState = ref.watch(fileImportStateProvider);
    if (importState.importing) {
      return Padding(
        padding: const EdgeInsets.all(24),
        child: ParticleWaveIntakeLoader(
          key: const ValueKey('local-file-import-particles'),
          progress: importState.progress,
          label: importState.label,
          sublabel: 'Subida local a 0% de pérdida · Preparando forma de onda',
        ),
      );
    }
    return Center(
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
            'o haz clic en Subir Audio para iniciar',
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
              label: 'Subir Audio',
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
    final targetMode = ref.watch(separationTargetModeProvider);
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
          height: isPhoneLayout(context) ? 168 : 238,
          child: _Panel(
            padding: EdgeInsets.all(isPhoneLayout(context) ? 14 : 18),
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
                      if (!isPhoneLayout(context) && audio.bitDepth != null) ...[
                        const SizedBox(width: 8),
                        _Tag('${audio.bitDepth}-bit'),
                      ],
                    ],
                    if (isPhoneLayout(context)) ...[
                      const SizedBox(width: 10),
                      _CyanButton(
                        label: targetMode == SeparationTargetMode.twoStems
                            ? 'Separar (2 stems Â· ~8-10m)'
                            : 'Separar (5 stems)',
                        height: 40,
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
                                      .separate(
                                        source.path,
                                        allowDownload: true,
                                        mode: targetMode == SeparationTargetMode.twoStems
                                            ? 'two_stems'
                                            : 'five_stems',
                                      );
                                }
                                : null,
                      ),
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
                          () => ParticleWaveIntakeLoader(
                            key: const ValueKey('waveform-preview-particles'),
                            progress: ref.watch(
                              sourcePreviewProgressProvider(source.path),
                            ),
                            label:
                                'Analizando archivo local y generando forma de onda…',
                            sublabel: source.name,
                            compact: isPhoneLayout(context),
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
                      audio == null ? '--:--' : formatStudioTime(audio.duration),
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
                      const _SectionLabel('MODO DE SEPARACIÓN'),
                      const SizedBox(height: 6),
                      const Text(
                        'Elige la cantidad de stems según tu necesidad y tiempo disponible',
                        style: TextStyle(fontSize: 10, color: AppColors.muted),
                      ),
                      const SizedBox(height: 12),
                      Row(
                        children: [
                          Expanded(
                            child: InkWell(
                              key: const ValueKey('mode-two-stems'),
                              borderRadius: BorderRadius.circular(10),
                              onTap: busy
                                  ? null
                                  : () => ref
                                      .read(separationTargetModeProvider.notifier)
                                      .state = SeparationTargetMode.twoStems,
                              child: Container(
                                padding: const EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: targetMode == SeparationTargetMode.twoStems
                                      ? AppColors.seaGlass.withValues(alpha: 0.12)
                                      : const Color(0xFF0F181F),
                                  borderRadius: BorderRadius.circular(10),
                                  border: Border.all(
                                    color: targetMode == SeparationTargetMode.twoStems
                                        ? AppColors.seaGlass
                                        : const Color(0xFF2A3A46),
                                    width: targetMode == SeparationTargetMode.twoStems ? 1.5 : 1,
                                  ),
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      children: [
                                        Icon(
                                          Icons.bolt,
                                          color: targetMode == SeparationTargetMode.twoStems
                                              ? AppColors.seaGlass
                                              : AppColors.muted,
                                          size: 18,
                                        ),
                                        const SizedBox(width: 6),
                                        Expanded(
                                          child: Text(
                                            '2 Stems (Rápido)',
                                            style: TextStyle(
                                              fontSize: 12,
                                              fontWeight: FontWeight.bold,
                                              color: targetMode == SeparationTargetMode.twoStems
                                                  ? AppColors.text
                                                  : AppColors.muted,
                                            ),
                                          ),
                                        ),
                                        Container(
                                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                          decoration: BoxDecoration(
                                            color: AppColors.seaGlass.withValues(alpha: 0.2),
                                            borderRadius: BorderRadius.circular(4),
                                          ),
                                          child: Text(
                                              isDesktopHost ? '~15-30 seg' : '~8-10 min',
                                              style: const TextStyle(
                                                  fontSize: 9,
                                                  color: AppColors.seaGlass,
                                                  fontWeight: FontWeight.bold)),
                                        ),
                                      ],
                                    ),
                                    const SizedBox(height: 6),
                                    const Text(
                                      'Voz limpia unificada + Instrumental puro.',
                                      style: TextStyle(fontSize: 10, color: AppColors.muted),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: InkWell(
                              key: const ValueKey('mode-five-stems'),
                              borderRadius: BorderRadius.circular(10),
                              onTap: busy
                                  ? null
                                  : () => ref
                                      .read(separationTargetModeProvider.notifier)
                                      .state = SeparationTargetMode.fiveStems,
                              child: Container(
                                padding: const EdgeInsets.all(12),
                                decoration: BoxDecoration(
                                  color: targetMode == SeparationTargetMode.fiveStems
                                      ? AppColors.seaGlass.withValues(alpha: 0.12)
                                      : const Color(0xFF0F181F),
                                  borderRadius: BorderRadius.circular(10),
                                  border: Border.all(
                                    color: targetMode == SeparationTargetMode.fiveStems
                                        ? AppColors.seaGlass
                                        : const Color(0xFF2A3A46),
                                    width: targetMode == SeparationTargetMode.fiveStems ? 1.5 : 1,
                                  ),
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      children: [
                                        Icon(
                                          Icons.equalizer,
                                          color: targetMode == SeparationTargetMode.fiveStems
                                              ? AppColors.seaGlass
                                              : AppColors.muted,
                                          size: 18,
                                        ),
                                        const SizedBox(width: 6),
                                        Expanded(
                                          child: Text(
                                            '5 Stems (Completo)',
                                            style: TextStyle(
                                              fontSize: 12,
                                              fontWeight: FontWeight.bold,
                                              color: targetMode == SeparationTargetMode.fiveStems
                                                  ? AppColors.text
                                                  : AppColors.muted,
                                            ),
                                          ),
                                        ),
                                        Container(
                                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                          decoration: BoxDecoration(
                                            color: Colors.amber.withValues(alpha: 0.2),
                                            borderRadius: BorderRadius.circular(4),
                                          ),
                                          child: Text(
                                              isDesktopHost ? '~1-2 min' : '30-40 min',
                                              style: const TextStyle(
                                                  fontSize: 9,
                                                  color: Colors.amber,
                                                  fontWeight: FontWeight.bold)),
                                        ),
                                      ],
                                    ),
                                    const SizedBox(height: 6),
                                    const Text(
                                      'Voz principal, Coros, Batería, Bajo e Instrumental.',
                                      style: TextStyle(fontSize: 10, color: AppColors.muted),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                      if (targetMode == SeparationTargetMode.fiveStems) ...[
                        Container(
                          margin: const EdgeInsets.only(top: 10),
                          padding: const EdgeInsets.all(10),
                          decoration: BoxDecoration(
                            color: Colors.amber.withValues(alpha: 0.10),
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(color: Colors.amber.withValues(alpha: 0.3)),
                          ),
                          child: Row(
                            children: [
                              const Icon(Icons.warning_amber_rounded, color: Colors.amber, size: 20),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  isDesktopHost
                                      ? 'Modo Estudio Completo PC: Ejecuta las 4 pasadas neuronales completas en precisión FP32 con 0% de pérdida (~1 a 2 minutos con GPU DirectML o ~6 minutos en CPU).'
                                      : 'Aviso importante: En teléfonos celulares este proceso realiza 4 pasadas neuronales y puede tomar entre 30 y 40 minutos dependiendo de la capacidad del dispositivo.',
                                  style: const TextStyle(fontSize: 10, color: Colors.amber, fontWeight: FontWeight.w500),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ] else ...[
                        Container(
                          margin: const EdgeInsets.only(top: 10),
                          padding: const EdgeInsets.all(10),
                          decoration: BoxDecoration(
                            color: AppColors.seaGlass.withValues(alpha: 0.10),
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(color: AppColors.seaGlass.withValues(alpha: 0.3)),
                          ),
                          child: Row(
                            children: [
                              const Icon(Icons.check_circle_outline, color: AppColors.seaGlass, size: 20),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  isDesktopHost
                                      ? 'Modo Rápido PC: Separa en 1 sola pasada neuronal (Voz e Instrumental puro) con aceleración DirectML/AVX2 y 0% de pérdida de calidad (FP32) en ~15 a 30 segundos.'
                                      : 'Modo Rápido: Separa en 1 sola pasada de estudio (Voz e Instrumental puro) con 0% de pérdida de calidad en ~8 a 10 minutos.',
                                  style: const TextStyle(fontSize: 10, color: AppColors.seaGlass, fontWeight: FontWeight.w500),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ],
                      const SizedBox(height: 12),
                      Text(
                        targetMode == SeparationTargetMode.twoStems
                            ? 'PISTAS RESULTANTES (2 STEMS)'
                            : 'PISTAS RESULTANTES (5 STEMS)',
                        style: const TextStyle(fontSize: 10, fontWeight: FontWeight.bold, color: AppColors.muted),
                      ),
                      const SizedBox(height: 6),
                      Expanded(
                        child: ListView(
                          children: [
                            if (targetMode == SeparationTargetMode.twoStems) ...[
                              _Panel(
                                color: const Color(0xFF0F181F),
                                radius: 8,
                                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                                child: Row(
                                  children: [
                                    Icon(stemIcon('lead_vocal'), color: stemColor('lead_vocal'), size: 20),
                                    const SizedBox(width: 10),
                                    const Expanded(
                                      child: Text('Voces completas (Principal y coros unificados limpios)',
                                          style: TextStyle(fontSize: 11, fontWeight: FontWeight.w600)),
                                    ),
                                  ],
                                ),
                              ),
                              const SizedBox(height: 6),
                              _Panel(
                                color: const Color(0xFF0F181F),
                                radius: 8,
                                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                                child: Row(
                                  children: [
                                    Icon(stemIcon('instrumental'), color: stemColor('instrumental'), size: 20),
                                    const SizedBox(width: 10),
                                    const Expanded(
                                      child: Text('Instrumental limpio (Base musical sin voces)',
                                          style: TextStyle(fontSize: 11, fontWeight: FontWeight.w600)),
                                    ),
                                  ],
                                ),
                              ),
                            ] else ...[
                              for (final kind in stemNames.keys)
                                Padding(
                                  padding: const EdgeInsets.only(bottom: 6),
                                  child: _Panel(
                                    color: const Color(0xFF0F181F),
                                    radius: 8,
                                    padding: EdgeInsets.zero,
                                    child: CheckboxListTile(
                                      value: selected.contains(kind),
                                      dense: true,
                                      controlAffinity: ListTileControlAffinity.leading,
                                      contentPadding: const EdgeInsets.symmetric(horizontal: 8),
                                      onChanged: busy
                                          ? null
                                          : (checked) {
                                              ref.read(selectedStemTypesProvider.notifier).state =
                                                  checked == true
                                                      ? {...selected, kind}
                                                      : ({...selected}..remove(kind));
                                            },
                                      title: Row(
                                        children: [
                                          Icon(stemIcon(kind), color: stemColor(kind), size: 20),
                                          const SizedBox(width: 10),
                                          Text(_stemLabel(kind),
                                              style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w600)),
                                        ],
                                      ),
                                    ),
                                  ),
                                ),
                            ],
                          ],
                        ),
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
                      const SizedBox(height: 12),
                      const ComputeSettingsPanel(compact: true),
                      const SizedBox(height: 18),
                      const _SummaryLine('Modelo', 'UVR / Kuielab · MDX'),
                      const _SummaryLine(
                        'Dispositivo',
                        'ONNX Runtime · procesamiento local',
                      ),
                      _SummaryLine(
                        'Calidad',
                        audio == null
                            ? 'Por analizar'
                            : _audioMeta(audio.sampleRate, audio.channels),
                      ),
                      _SummaryLine(
                        'Duración estimada',
                        isDesktopHost
                            ? (targetMode == SeparationTargetMode.twoStems
                                ? '~15-30 seg (GPU) / ~1.5 min (CPU)'
                                : '~1-2 min (GPU) / ~6 min (CPU)')
                            : (targetMode == SeparationTargetMode.twoStems
                                ? '~8-10 min'
                                : '~30-40 min'),
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
                        label: targetMode == SeparationTargetMode.twoStems
                            ? (isDesktopHost
                                ? 'Separar en 2 stems (Rápido ~15-30 seg)'
                                : 'Separar en 2 stems (Rápido ~8-10 min)')
                            : (isDesktopHost
                                ? 'Separar en 5 stems (Completo ~1-2 min)'
                                : 'Separar en 5 stems (Completo ~30-40 min)'),
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
                                      .separate(
                                        source.path,
                                        allowDownload: !isDesktopHost,
                                        mode: targetMode == SeparationTargetMode.twoStems
                                            ? 'two_stems'
                                            : 'five_stems',
                                      );
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
