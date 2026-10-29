part of 'audio_workbench.dart';

final _projectSearchProvider = StateProvider<String>((ref) => '');

class _SessionHeader extends ConsumerWidget {
  const _SessionHeader({required this.onImport});
  final VoidCallback onImport;
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final view = ref.watch(studioViewProvider);
    final studio = ref.watch(studioProvider);
    final source = ref.watch(workbenchProvider).selectedSource;
    final flow = ref.watch(separationFlowProvider) && studio.busy;
    final result = ref.watch(separationResultProvider);
    if (view == StudioView.mixer) return const _MixerSessionHeader();
    final title = switch (view) {
      StudioView.home => 'Inicio',
      StudioView.settings => 'Configuración',
      StudioView.atomizer => 'Atomizador',
      StudioView.mixer => studio.session?.name ?? 'Mezclador',
    };
    final subtitle = switch (view) {
      StudioView.home => 'Tus proyectos y mezclas recientes',
      StudioView.settings =>
        'Ajustes del estudio, la interfaz y el motor de separación',
      StudioView.atomizer =>
        flow
            ? 'Separando ${source?.name ?? studio.session?.name ?? "audio"}'
            : result
            ? '${studio.session?.name ?? "Audio"} · separación terminada'
            : source != null
            ? 'Revisa el audio y confirma la separación en 5 stems'
            : 'Convierte una mezcla en 5 stems listos para editar',
      StudioView.mixer => '5 stems sincronizados · procesamiento local',
    };
    return Container(
      height: 66,
      padding: const EdgeInsets.fromLTRB(96, 8, 16, 8),
      decoration: const BoxDecoration(
        color: Color(0xFF0F151C),
        border: Border(bottom: BorderSide(color: Color(0xFF1A2531))),
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  subtitle,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontSize: 9.8,
                    height: 1.2,
                    color: AppColors.muted,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 18),
          if (view == StudioView.home) ...[
            SizedBox(
              width: MediaQuery.sizeOf(context).width < 1150 ? 180 : 280,
              height: 36,
              child: TextField(
                onChanged:
                    (v) => ref.read(_projectSearchProvider.notifier).state = v,
                style: const TextStyle(fontSize: 11),
                decoration: const InputDecoration(
                  hintText: 'Buscar proyectos, audio…',
                  prefixIcon: Icon(Icons.search, size: 17),
                  contentPadding: EdgeInsets.zero,
                ),
              ),
            ),
            const SizedBox(width: 16),
            _CyanButton(
              label: 'Importar audio',
              icon: Icons.file_upload_outlined,
              onPressed: onImport,
            ),
          ] else if (view == StudioView.atomizer) ...[
            _Tag(
              flow
                  ? 'En curso · ${((studio.job?.progress ?? 0) * 100).round()}%'
                  : result
                  ? 'Separación lista'
                  : source != null
                  ? 'Listo para separar'
                  : 'Sin audio cargado',
              color:
                  source != null || result || flow
                      ? AppColors.seaGlass
                      : AppColors.muted,
            ),
            const SizedBox(width: 12),
            if (flow) ...[
              OutlinedButton(
                onPressed: () => windowManager.minimize(),
                child: const Text('Minimizar'),
              ),
              const SizedBox(width: 8),
              OutlinedButton(
                onPressed: () => ref.read(studioProvider.notifier).cancel(),
                child: const Text('Cancelar'),
              ),
            ] else
              OutlinedButton(
                onPressed:
                    source != null && !result
                        ? onImport
                        : () =>
                            ref.read(studioViewProvider.notifier).state =
                                StudioView.home,
                child: Text(
                  source != null && !result ? 'Cambiar audio' : 'Historial',
                ),
              ),
          ] else ...[
            OutlinedButton(
              onPressed: () {
                ref.read(studioTextScaleProvider.notifier).state = 1;
                ref.read(studioDensityProvider.notifier).state = 1;
                ref.read(studioGridProvider.notifier).state = true;
                ref.read(studioReducedMotionProvider.notifier).state = true;
              },
              child: const Text('Restablecer'),
            ),
            const SizedBox(width: 12),
            _CyanButton(
              label: 'Guardar cambios',
              icon: Icons.check,
              onPressed: () async {
                try {
                  await saveDesktopPreferences(ref);
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('Ajustes guardados.')),
                    );
                  }
                } on FileSystemException catch (error) {
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(
                        content: Text(
                          'No se pudieron guardar los ajustes: ${error.message}',
                        ),
                      ),
                    );
                  }
                }
              },
            ),
          ],
        ],
      ),
    );
  }
}

class _Panel extends StatelessWidget {
  const _Panel({
    required this.child,
    this.padding = const EdgeInsets.all(20),
    this.color,
    this.radius = 14,
  });
  final Widget child;
  final EdgeInsetsGeometry padding;
  final Color? color;
  final double radius;
  @override
  Widget build(BuildContext context) => Material(
    color: color ?? AppColors.panel,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(radius),
      side: const BorderSide(color: Color(0xFF1E2A35)),
    ),
    child: Padding(padding: padding, child: child),
  );
}

class _CyanButton extends StatelessWidget {
  const _CyanButton({
    required this.label,
    required this.onPressed,
    this.icon = Icons.auto_awesome,
    this.height = 38,
  });
  final String label;
  final VoidCallback? onPressed;
  final IconData icon;
  final double height;
  @override
  Widget build(BuildContext context) => SizedBox(
    height: height,
    child: FilledButton.icon(
      onPressed: onPressed,
      icon: Icon(icon, size: 17),
      style: FilledButton.styleFrom(
        padding: const EdgeInsets.symmetric(horizontal: 20),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      ),
      label: Text(
        label,
        style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
      ),
    ),
  );
}

class _Tag extends StatelessWidget {
  const _Tag(this.text, {this.color = AppColors.muted});
  final String text;
  final Color color;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
    decoration: BoxDecoration(
      color: color.withValues(alpha: .09),
      border: Border.all(color: color.withValues(alpha: .18)),
      borderRadius: BorderRadius.circular(20),
    ),
    child: Text(
      text,
      style: TextStyle(fontSize: 9, fontWeight: FontWeight.w600, color: color),
    ),
  );
}

class _SectionLabel extends StatelessWidget {
  const _SectionLabel(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Text(
    text,
    style: const TextStyle(
      fontSize: 9,
      letterSpacing: 1.1,
      color: AppColors.muted,
      fontWeight: FontWeight.w700,
    ),
  );
}

String _audioMeta(int rate, int channels) =>
    '${rate > 0 ? "${(rate / 1000).toStringAsFixed(rate % 1000 == 0 ? 0 : 1)} kHz" : "— kHz"} · ${channels == 1 ? "Mono" : "Estéreo"}';
String _fileSize(int size) =>
    size <= 0
        ? 'Tamaño no disponible'
        : '${(size / 1048576).toStringAsFixed(1)} MB';
String _stemLabel(String kind) => switch (kind) {
  'lead_vocal' => 'Voz principal',
  'backing_vocals' => 'Coros',
  'drums' => 'Batería',
  'bass' => 'Bajo',
  _ => 'Instrumental',
};
Future<void> _openProject(WidgetRef ref, String directory) async {
  await ref.read(studioProvider.notifier).open(directory);
  if (ref.context.mounted && ref.read(studioProvider).error == null) {
    ref.read(studioViewProvider.notifier).state = StudioView.mixer;
  }
}

Future<void> _showSessionFolder(BuildContext context, String directory) =>
    openStudioFolder(context, directory);

class _MixerSessionHeader extends ConsumerWidget {
  const _MixerSessionHeader();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final track = state.session?.tracks.firstOrNull;
    return Container(
      height: 66,
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      decoration: const BoxDecoration(
        color: Color(0xFF0F151C),
        border: Border(bottom: BorderSide(color: Color(0xFF1A2531))),
      ),
      child: Row(
        children: [
          SizedBox(
            width: MediaQuery.sizeOf(context).width >= 1200 ? 520 : 400,
            child: _Panel(
              radius: 10,
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 6),
              child: Row(
                children: [
                  const Icon(
                    Icons.graphic_eq,
                    size: 20,
                    color: AppColors.seaGlass,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          state.session?.name ?? 'Mezclador',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontSize: 12.5,
                            fontWeight: FontWeight.w600,
                            height: 1.2,
                          ),
                        ),
                        const SizedBox(height: 4),
                        const Text(
                          'Sesión local · stems separados',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 9.3,
                            height: 1.2,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                  if (track != null)
                    _Tag(_audioMeta(track.sampleRate, track.channels)),
                ],
              ),
            ),
          ),
          const Spacer(),
          const _Tag('Sesión cargada', color: AppColors.seaGlass),
          const SizedBox(width: 18),
          _CyanButton(
            label: 'Nuevo proyecto',
            icon: Icons.add,
            onPressed:
                state.busy
                    ? null
                    : () {
                      if (state.playing) {
                        ref.read(studioProvider.notifier).transport('pause');
                      }
                      ref.read(workbenchProvider.notifier).clearSources();
                      ref.read(separationResultProvider.notifier).state = false;
                      ref.read(studioViewProvider.notifier).state =
                          StudioView.atomizer;
                    },
          ),
        ],
      ),
    );
  }
}
