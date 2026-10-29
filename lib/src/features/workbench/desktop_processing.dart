part of 'audio_workbench.dart';

class _SeparationProgress extends ConsumerWidget {
  const _SeparationProgress();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final source = ref.watch(workbenchProvider).selectedSource;
    final preview =
        source == null
            ? null
            : ref.watch(sourceAudioPreviewProvider(source.path)).valueOrNull;
    final progress = (state.job?.progress ?? 0).clamp(0.0, 1.0);
    final stage =
        progress < .15
            ? 0
            : progress < .43
            ? 1
            : progress < .85
            ? 2
            : progress < .95
            ? 3
            : 4;
    const stages = [
      ('Analizando el audio', 'Duración, canales y nivel de entrada'),
      ('Detectando voces', 'Separando voz principal y coros'),
      ('Separando los instrumentos', 'Batería, bajo e instrumental'),
      ('Validando reconstrucción', 'Comprobando la alineación de las pistas'),
      ('Verificando y generando stems', 'Guardando los archivos finales'),
    ];
    return _PageViewport(
      minHeight: 720,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _Panel(
            padding: const EdgeInsets.all(24),
            child: SizedBox(
              height: 210,
              child: Row(
                children: [
                  Expanded(
                    flex: 4,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const _SectionLabel('ENTRADA · MEZCLA ORIGINAL'),
                        const SizedBox(height: 24),
                        Expanded(
                          child:
                              preview == null
                                  ? const Center(
                                    child: Icon(
                                      Icons.graphic_eq,
                                      size: 60,
                                      color: AppColors.muted,
                                    ),
                                  )
                                  : CustomPaint(
                                    painter: _AudioPreviewPainter(
                                      preview.peaks,
                                      AppColors.muted,
                                    ),
                                  ),
                        ),
                      ],
                    ),
                  ),
                  const Padding(
                    padding: EdgeInsets.symmetric(horizontal: 36),
                    child: Icon(
                      Icons.call_split,
                      size: 44,
                      color: AppColors.seaGlass,
                    ),
                  ),
                  Expanded(
                    flex: 6,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const _SectionLabel('SALIDA · 5 STEMS'),
                        const SizedBox(height: 14),
                        for (final kind in stemNames.keys)
                          Expanded(
                            child: Row(
                              children: [
                                SizedBox(
                                  width: 104,
                                  child: Text(
                                    _stemLabel(kind),
                                    style: TextStyle(
                                      fontSize: 10,
                                      color: stemColor(kind),
                                    ),
                                  ),
                                ),
                                Expanded(
                                  child: LinearProgressIndicator(
                                    value: progress,
                                    color: stemColor(kind),
                                    backgroundColor: stemColor(
                                      kind,
                                    ).withValues(alpha: .07),
                                    minHeight: 3,
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
          ),
          const SizedBox(height: 22),
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Etapa ${stage + 1} de 5 · ${stages[stage].$1}',
                      style: const TextStyle(
                        fontSize: 17,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      '${(progress * 100).round()} % completado · puedes minimizar la ventana',
                      style: const TextStyle(
                        fontSize: 11,
                        color: AppColors.muted,
                      ),
                    ),
                  ],
                ),
              ),
              Text(
                '${(progress * 100).round()} %',
                style: const TextStyle(
                  fontSize: 32,
                  color: AppColors.seaGlass,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ],
          ),
          const SizedBox(height: 16),
          LinearProgressIndicator(value: progress, minHeight: 6),
          const SizedBox(height: 24),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  flex: 6,
                  child: _Panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const _SectionLabel('ETAPAS DEL PROCESO'),
                        const SizedBox(height: 12),
                        for (var i = 0; i < stages.length; i++)
                          Expanded(
                            child: Row(
                              children: [
                                CircleAvatar(
                                  radius: 15,
                                  backgroundColor:
                                      i <= stage
                                          ? AppColors.seaGlass.withValues(
                                            alpha: .10,
                                          )
                                          : AppColors.canvas,
                                  child:
                                      i < stage
                                          ? const Icon(
                                            Icons.check,
                                            size: 16,
                                            color: AppColors.seaGlass,
                                          )
                                          : Text(
                                            '${i + 1}',
                                            style: TextStyle(
                                              fontSize: 10,
                                              color:
                                                  i == stage
                                                      ? AppColors.seaGlass
                                                      : AppColors.muted,
                                            ),
                                          ),
                                ),
                                const SizedBox(width: 14),
                                Expanded(
                                  child: Column(
                                    mainAxisAlignment: MainAxisAlignment.center,
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        stages[i].$1,
                                        style: TextStyle(
                                          fontSize: 11,
                                          color:
                                              i == stage
                                                  ? AppColors.text
                                                  : AppColors.muted,
                                        ),
                                      ),
                                      const SizedBox(height: 5),
                                      Text(
                                        stages[i].$2,
                                        style: const TextStyle(
                                          fontSize: 9,
                                          color: AppColors.muted,
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                                _Tag(
                                  i < stage
                                      ? 'Listo'
                                      : i == stage
                                      ? 'En curso'
                                      : 'En espera',
                                  color:
                                      i <= stage
                                          ? AppColors.seaGlass
                                          : AppColors.muted,
                                ),
                              ],
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(width: 24),
                const Expanded(
                  flex: 4,
                  child: _Panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        _SectionLabel('MIENTRAS ESPERAS'),
                        SizedBox(height: 28),
                        _WaitHint(
                          Icons.desktop_windows_outlined,
                          'No cierres la app: la separación sigue en segundo plano',
                        ),
                        SizedBox(height: 22),
                        _WaitHint(
                          Icons.notifications_none,
                          'Verás el resultado al terminar',
                        ),
                        SizedBox(height: 22),
                        _WaitHint(
                          Icons.shield_outlined,
                          'El audio no sale de tu equipo',
                        ),
                        Spacer(),
                        Text(
                          'La duración depende del audio y del rendimiento de tu equipo.',
                          style: TextStyle(
                            fontSize: 10,
                            height: 1.6,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 14),
          const Text(
            'Puedes cancelar ahora; la sesión anterior se conserva.',
            style: TextStyle(fontSize: 10, color: AppColors.muted),
          ),
        ],
      ),
    );
  }
}

class _WaitHint extends StatelessWidget {
  const _WaitHint(this.icon, this.text);
  final IconData icon;
  final String text;
  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Icon(icon, size: 18, color: AppColors.seaGlass),
      const SizedBox(width: 12),
      Expanded(
        child: Text(
          text,
          style: const TextStyle(
            fontSize: 11,
            height: 1.6,
            color: AppColors.muted,
          ),
        ),
      ),
    ],
  );
}

class _SeparationResult extends ConsumerWidget {
  const _SeparationResult();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final session = state.session!;
    return _PageViewport(
      minHeight: 720,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const SizedBox(height: 14),
          Center(
            child: Container(
              width: 84,
              height: 84,
              decoration: const BoxDecoration(
                shape: BoxShape.circle,
                color: AppColors.seaGlass,
              ),
              child: const Icon(
                Icons.check,
                size: 38,
                color: Color(0xFF04231F),
              ),
            ),
          ),
          const SizedBox(height: 24),
          Text(
            'Tus ${session.tracks.length} stems están listos',
            textAlign: TextAlign.center,
            style: const TextStyle(
              fontSize: 26,
              fontWeight: FontWeight.w700,
              letterSpacing: -.6,
            ),
          ),
          const SizedBox(height: 10),
          Text(
            '${session.name} · ${formatStudioTime(state.duration)} · separado en este equipo',
            textAlign: TextAlign.center,
            style: const TextStyle(fontSize: 11, color: AppColors.muted),
          ),
          const SizedBox(height: 30),
          SizedBox(
            height: 96,
            child: Row(
              children: [
                for (final track in session.tracks) ...[
                  if (track != session.tracks.first) const SizedBox(width: 14),
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: AppColors.panel,
                        borderRadius: BorderRadius.circular(8),
                        border: Border(
                          left: BorderSide(
                            color: stemColor(track.stemType),
                            width: 3,
                          ),
                        ),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            _stemLabel(track.stemType),
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 11,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 5),
                          Text(
                            '${formatStudioTime(track.durationSeconds)} · ${(track.sampleRate / 1000).toStringAsFixed(1)} kHz',
                            style: const TextStyle(
                              fontSize: 9,
                              color: AppColors.muted,
                            ),
                          ),
                          const SizedBox(height: 8),
                          Expanded(
                            child: SizedBox(
                              width: double.infinity,
                              child: CustomPaint(
                                painter: _AudioPreviewPainter(
                                  track.peaks,
                                  stemColor(track.stemType),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(height: 22),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              _CyanButton(
                label: 'Abrir mezclador',
                icon: Icons.tune,
                height: 48,
                onPressed:
                    () =>
                        ref.read(studioViewProvider.notifier).state =
                            StudioView.mixer,
              ),
              const SizedBox(width: 16),
              OutlinedButton.icon(
                onPressed:
                    () => showDialog<void>(
                      context: context,
                      builder: (_) => const ExportDialog(),
                    ),
                icon: const Icon(Icons.file_download_outlined, size: 17),
                label: const Text('Exportar stems'),
              ),
              const SizedBox(width: 16),
              TextButton.icon(
                onPressed: () => _showSessionFolder(context, session.directory),
                icon: const Icon(Icons.folder_open_outlined, size: 17),
                label: const Text('Ver carpeta'),
              ),
            ],
          ),
          const SizedBox(height: 26),
          Row(
            children: [
              const Expanded(child: _SectionLabel('RESULTADO EN EL MEZCLADOR')),
              TextButton(
                onPressed:
                    () =>
                        ref.read(studioViewProvider.notifier).state =
                            StudioView.mixer,
                child: const Text(
                  'Abrir mezclador →',
                  style: TextStyle(fontSize: 10),
                ),
              ),
            ],
          ),
          Expanded(
            child: _Panel(
              padding: const EdgeInsets.all(14),
              child: Column(
                children: [
                  for (final track in session.tracks)
                    Expanded(
                      child: Row(
                        children: [
                          Icon(
                            stemIcon(track.stemType),
                            size: 18,
                            color: stemColor(track.stemType),
                          ),
                          const SizedBox(width: 10),
                          SizedBox(
                            width: 105,
                            child: Text(
                              _stemLabel(track.stemType),
                              style: const TextStyle(fontSize: 10),
                            ),
                          ),
                          Expanded(
                            child: Padding(
                              padding: const EdgeInsets.symmetric(vertical: 7),
                              child: SizedBox(
                                height: double.infinity,
                                child: CustomPaint(
                                  painter: _AudioPreviewPainter(
                                    track.peaks,
                                    stemColor(track.stemType),
                                    progress:
                                        state.duration > 0
                                            ? state.position / state.duration
                                            : 0,
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 26),
        ],
      ),
    );
  }
}
