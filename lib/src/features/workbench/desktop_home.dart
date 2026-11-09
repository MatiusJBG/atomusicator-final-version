part of 'audio_workbench.dart';

final _projectFilterProvider = StateProvider<int>((ref) => 0);

class _ProjectsHome extends ConsumerWidget {
  const _ProjectsHome({required this.onImport});
  final VoidCallback onImport;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final projects = ref.watch(studioProjectsProvider);
    final search = ref.watch(_projectSearchProvider).toLowerCase();
    final filter = ref.watch(_projectFilterProvider);
    final favorites = ref.watch(favoriteProjectsProvider);
    final items =
        (projects.valueOrNull ?? [])
            .where(
              (p) =>
                  p.name.toLowerCase().contains(search) &&
                  (filter != 2 || favorites.contains(p.directory)),
            )
            .take(filter == 1 ? 3 : 30)
            .toList();
    return _PageViewport(
      minHeight: 680,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const StudioFeedback(),
          const _SectionLabel('TUS PROYECTOS'),
          const SizedBox(height: 8),
          Text(
            '${items.length} proyectos · separación en 5 stems',
            style: const TextStyle(fontSize: 10, color: AppColors.muted),
          ),
          const SizedBox(height: 16),
          Row(
            children: [
              for (var i = 0; i < 3; i++) ...[
                if (i > 0) const SizedBox(width: 10),
                ChoiceChip(
                  label: Text(
                    ['Todos', 'Recientes', 'Favoritos'][i],
                    style: const TextStyle(fontSize: 10),
                  ),
                  selected: filter == i,
                  showCheckmark: false,
                  onSelected:
                      (_) =>
                          ref.read(_projectFilterProvider.notifier).state = i,
                ),
              ],
              const Spacer(),
              OutlinedButton.icon(
                onPressed:
                    ref.watch(studioProvider.select((s) => s.busy))
                        ? null
                        : () async {
                          await openStudioSession(context, ref);
                          if (context.mounted &&
                              ref.read(studioProvider).session != null) {
                            ref.read(studioViewProvider.notifier).state =
                                StudioView.mixer;
                          }
                        },
                icon: const Icon(Icons.folder_open_outlined, size: 15),
                label: const Text('Abrir proyecto'),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Expanded(
            flex: 5,
            child:
                projects.isLoading
                    ? const Center(
                      child: ParticleBurstLoader(
                        size: 46,
                        label: 'Cargando proyectos del estudio…',
                      ),
                    )
                    : items.isEmpty
                    ? _Panel(
                      child: Center(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            const Icon(
                              Icons.folder_open_outlined,
                              size: 38,
                              color: AppColors.muted,
                            ),
                            const SizedBox(height: 18),
                            Text(
                              search.isNotEmpty || filter == 2
                                  ? 'No hay proyectos que coincidan'
                                  : 'Tus proyectos aparecerán aquí',
                              style: const TextStyle(
                                fontSize: 17,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 10),
                            Text(
                              projects.hasError
                                  ? 'No se pudo leer la carpeta de proyectos. Puedes abrir una sesión existente.'
                                  : 'Importa una mezcla para crear tu primera separación.',
                              style: const TextStyle(
                                fontSize: 11,
                                color: AppColors.muted,
                              ),
                            ),
                          ],
                        ),
                      ),
                    )
                    : LayoutBuilder(
                      builder:
                          (context, box) => GridView.builder(
                            itemCount: items.length,
                            gridDelegate:
                                SliverGridDelegateWithFixedCrossAxisCount(
                                  crossAxisCount:
                                      box.maxWidth < 560
                                          ? 1
                                          : (box.maxWidth < 1000 ? 2 : 3),
                                  mainAxisExtent: math.max(box.maxHeight, 252.0),
                                  crossAxisSpacing: 24,
                                  mainAxisSpacing: 16,
                                ),
                            itemBuilder:
                                (context, i) => _ProjectCard(
                                  project: items[i],
                                  color: stemColor(
                                    ['lead_vocal', 'drums', 'bass'][i % 3],
                                  ),
                                ),
                          ),
                    ),
          ),
          const SizedBox(height: 26),
          const _SectionLabel('ACTIVIDAD RECIENTE'),
          const SizedBox(height: 14),
          Expanded(
            flex: 3,
            child:
                items.isEmpty
                    ? const Align(
                      alignment: Alignment.topLeft,
                      child: Text(
                        'Aún no hay separaciones guardadas.',
                        style: TextStyle(fontSize: 11, color: AppColors.muted),
                      ),
                    )
                    : ListView.separated(
                      itemCount: items.length > 3 ? 3 : items.length,
                      separatorBuilder: (_, __) => const SizedBox(height: 8),
                      itemBuilder:
                          (_, i) => _Panel(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 16,
                              vertical: 10,
                            ),
                            radius: 8,
                            child: Row(
                              children: [
                                Icon(
                                  Icons.graphic_eq,
                                  color: stemColor(
                                    ['lead_vocal', 'drums', 'bass'][i % 3],
                                  ),
                                  size: 22,
                                ),
                                const SizedBox(width: 14),
                                Expanded(
                                  child: Text(
                                    items[i].name,
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(
                                      fontSize: 11,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                ),
                                Text(
                                  formatStudioTime(items[i].duration),
                                  style: const TextStyle(
                                    fontSize: 10,
                                    color: AppColors.muted,
                                  ),
                                ),
                                const SizedBox(width: 30),
                                TextButton(
                                  onPressed:
                                      () =>
                                          _openProject(ref, items[i].directory),
                                  child: const Text(
                                    'Abrir mezclador',
                                    style: TextStyle(fontSize: 10),
                                  ),
                                ),
                                IconButton(
                                  tooltip: 'Eliminar proyecto',
                                  icon: const Icon(
                                    Icons.delete_outline_rounded,
                                    size: 16,
                                    color: AppColors.muted,
                                  ),
                                  onPressed:
                                      () => _confirmDeleteProject(
                                        context,
                                        ref,
                                        items[i],
                                      ),
                                ),
                              ],
                            ),
                          ),
                    ),
          ),
          const SizedBox(height: 20),
          _Panel(
            color: const Color(0xFF102D29),
            padding: const EdgeInsets.all(20),
            child: Row(
              children: [
                const Icon(
                  Icons.auto_awesome,
                  color: AppColors.seaGlass,
                  size: 24,
                ),
                const SizedBox(width: 20),
                const Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '¿Empezar una separación nueva?',
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      SizedBox(height: 6),
                      Text(
                        'Arrastra un audio a la ventana o abre el Atomizador: separa voz, coros, batería, bajo e instrumental.',
                        style: TextStyle(fontSize: 9.5, color: AppColors.muted),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 20),
                _CyanButton(
                  label: 'Ir al Atomizador',
                  icon: Icons.auto_awesome,
                  onPressed:
                      () =>
                          ref.read(studioViewProvider.notifier).state =
                              StudioView.atomizer,
                ),
              ],
            ),
          ),
          const SizedBox(height: 32),
        ],
      ),
    );
  }
}

class _ProjectCard extends ConsumerWidget {
  const _ProjectCard({required this.project, required this.color});
  final StudioProject project;
  final Color color;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final favorite = ref
        .watch(favoriteProjectsProvider)
        .contains(project.directory);
    final preview =
        ref
            .watch(
              sourceAudioPreviewProvider('${project.directory}/lead_vocal.wav'),
            )
            .valueOrNull;
    return _Panel(
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
              color: AppColors.canvas,
              child: Column(
                children: [
                  Row(
                    children: [
                      _Tag('${project.stems} stems', color: AppColors.seaGlass),
                      const Spacer(),
                      _Tag(formatStudioTime(project.duration)),
                    ],
                  ),
                  const SizedBox(height: 6),
                  Expanded(
                    child: SizedBox(
                      width: double.infinity,
                      child: Center(
                        child: preview == null
                            ? FittedBox(
                                fit: BoxFit.scaleDown,
                                child: Icon(
                                  Icons.graphic_eq_rounded,
                                  color: color.withValues(alpha: .4),
                                  size: 38,
                                ),
                              )
                            : CustomPaint(
                                size: Size.infinite,
                                painter: _AudioPreviewPainter(
                                  preview.peaks,
                                  color,
                                ),
                              ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  project.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 12.6,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  '${formatStudioTime(project.duration)} · ${project.stems} stems',
                  style: const TextStyle(fontSize: 9, color: AppColors.muted),
                ),
                const SizedBox(height: 8),
                _Tag(_audioMeta(project.sampleRate, project.channels)),
                const SizedBox(height: 8),
                Row(
                  children: [
                    OutlinedButton(
                      onPressed: () => _openProject(ref, project.directory),
                      child: const Text('Abrir'),
                    ),
                    const Spacer(),
                    IconButton(
                      tooltip: 'Eliminar proyecto',
                      icon: const Icon(
                        Icons.delete_outline_rounded,
                        size: 17,
                        color: AppColors.muted,
                      ),
                      onPressed:
                          () => _confirmDeleteProject(context, ref, project),
                    ),
                    IconButton(
                      tooltip:
                          favorite
                              ? 'Quitar de favoritos'
                              : 'Añadir a favoritos',
                      icon: Icon(
                        favorite ? Icons.star : Icons.star_outline,
                        size: 17,
                        color: favorite ? AppColors.amber : AppColors.muted,
                      ),
                      onPressed: () {
                        final next = {...ref.read(favoriteProjectsProvider)};
                        if (!next.remove(project.directory)) {
                          next.add(project.directory);
                        }
                        ref.read(favoriteProjectsProvider.notifier).state =
                            next;
                      },
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

Future<void> _confirmDeleteProject(
  BuildContext context,
  WidgetRef ref,
  StudioProject project,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    barrierDismissible: true,
    builder: (dialogContext) => Dialog(
      backgroundColor: AppColors.panel,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(14),
        side: const BorderSide(color: AppColors.line),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 520),
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Container(
                    width: 38,
                    height: 38,
                    decoration: BoxDecoration(
                      color: AppColors.coral.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: const Icon(
                      Icons.delete_forever_rounded,
                      color: AppColors.coral,
                      size: 22,
                    ),
                  ),
                  const SizedBox(width: 14),
                  const Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '¿Eliminar proyecto?',
                          style: TextStyle(
                            fontSize: 17,
                            fontWeight: FontWeight.w700,
                            color: AppColors.text,
                          ),
                        ),
                        SizedBox(height: 2),
                        Text(
                          'Esta acción no se puede deshacer',
                          style: TextStyle(
                            fontSize: 11,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close, size: 18),
                    onPressed: () => Navigator.of(dialogContext).pop(false),
                  ),
                ],
              ),
              const SizedBox(height: 20),
              Container(
                padding: const EdgeInsets.all(14),
                decoration: BoxDecoration(
                  color: AppColors.canvas,
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: AppColors.line),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      project.name,
                      style: const TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w700,
                        color: AppColors.text,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '${project.stems} stems · ${formatStudioTime(project.duration)}',
                      style: const TextStyle(
                        fontSize: 10,
                        color: AppColors.muted,
                      ),
                    ),
                    const SizedBox(height: 12),
                    const Text(
                      'CARPETA EN DISCO IDENTIFICADA',
                      style: TextStyle(
                        fontSize: 9.5,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.7,
                        color: AppColors.muted,
                      ),
                    ),
                    const SizedBox(height: 6),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 8,
                      ),
                      decoration: BoxDecoration(
                        color: AppColors.panelRaised,
                        borderRadius: BorderRadius.circular(6),
                        border: Border.all(color: AppColors.line),
                      ),
                      child: Row(
                        children: [
                          const Icon(
                            Icons.folder_outlined,
                            size: 16,
                            color: AppColors.amber,
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: SelectableText(
                              project.directory,
                              style: const TextStyle(
                                fontSize: 10.5,
                                fontFamily: 'monospace',
                                color: AppColors.text,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 14),
              const Text(
                'Al confirmar, se eliminará el proyecto y su carpeta física con todos los archivos de stems sincronizados en disco.',
                style: TextStyle(
                  fontSize: 11,
                  color: AppColors.muted,
                  height: 1.4,
                ),
              ),
              const SizedBox(height: 24),
              Wrap(
                alignment: WrapAlignment.end,
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: 12,
                runSpacing: 8,
                children: [
                  TextButton(
                    onPressed: () => Navigator.of(dialogContext).pop(false),
                    child: const Text('Cancelar'),
                  ),
                  ElevatedButton.icon(
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppColors.coral,
                      foregroundColor: Colors.white,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 12,
                      ),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(8),
                      ),
                    ),
                    icon: const Icon(Icons.delete_rounded, size: 16),
                    label: const Text(
                      'Eliminar proyecto y carpeta',
                      style: TextStyle(
                        fontSize: 11.5,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    onPressed: () => Navigator.of(dialogContext).pop(true),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    ),
  );

  if (confirmed == true && context.mounted) {
    final success = await deleteStudioProject(ref, project);
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            success
                ? 'Proyecto "${project.name}" y su carpeta eliminados correctamente.'
                : 'No se pudo eliminar la carpeta del proyecto.',
          ),
          backgroundColor: success ? AppColors.panelRaised : AppColors.coral,
        ),
      );
    }
  }
}
