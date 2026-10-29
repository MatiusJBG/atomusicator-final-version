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
                    ? const Center(child: CircularProgressIndicator())
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
                                  crossAxisCount: box.maxWidth < 1000 ? 2 : 3,
                                  mainAxisExtent: box.maxHeight,
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
              padding: const EdgeInsets.all(16),
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
                  const SizedBox(height: 10),
                  Expanded(
                    child: SizedBox(
                      width: double.infinity,
                      child:
                          preview == null
                              ? Icon(
                                Icons.graphic_eq,
                                color: color.withValues(alpha: .4),
                                size: 42,
                              )
                              : CustomPaint(
                                painter: _AudioPreviewPainter(
                                  preview.peaks,
                                  color,
                                ),
                              ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(16),
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
                const SizedBox(height: 6),
                Text(
                  '${formatStudioTime(project.duration)} · ${project.stems} stems',
                  style: const TextStyle(fontSize: 9, color: AppColors.muted),
                ),
                const SizedBox(height: 10),
                _Tag(_audioMeta(project.sampleRate, project.channels)),
                const SizedBox(height: 10),
                Row(
                  children: [
                    OutlinedButton(
                      onPressed: () => _openProject(ref, project.directory),
                      child: const Text('Abrir'),
                    ),
                    const Spacer(),
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
