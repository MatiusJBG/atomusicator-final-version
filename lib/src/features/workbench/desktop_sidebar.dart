part of 'audio_workbench.dart';

class _DesktopTitleBar extends ConsumerWidget {
  const _DesktopTitleBar();
  @override
  Widget build(BuildContext context, WidgetRef ref) => SizedBox(
    height: 38,
    child: Row(
      children: [
        Expanded(
          child: DragToMoveArea(
            child: Padding(
              padding: const EdgeInsets.only(left: 24),
              child: Row(
                children: [
                  const Icon(
                    Icons.graphic_eq_rounded,
                    size: 18,
                    color: AppColors.seaGlass,
                  ),
                  const SizedBox(width: 8),
                  const Text(
                    'Atomusicator',
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(width: 14),
                  Flexible(
                    child: Text(
                      ref.watch(
                            studioProvider.select((s) => s.session?.name),
                          ) ??
                          'Atomusicator',
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontSize: 11,
                        color: AppColors.muted,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        _WindowButton(
          icon: Icons.remove,
          label: 'Minimizar',
          action: windowManager.minimize,
        ),
        _WindowButton(
          icon: Icons.crop_square,
          label: 'Maximizar',
          action: () async {
            if (await windowManager.isMaximized()) {
              await windowManager.unmaximize();
            } else {
              await windowManager.maximize();
            }
          },
        ),
        _WindowButton(
          icon: Icons.close,
          label: 'Cerrar',
          action: windowManager.close,
        ),
      ],
    ),
  );
}

class _WindowButton extends StatelessWidget {
  const _WindowButton({
    required this.icon,
    required this.label,
    required this.action,
  });
  final IconData icon;
  final String label;
  final VoidCallback action;
  @override
  Widget build(BuildContext context) => SizedBox(
    width: 44,
    height: 34,
    child: IconButton(
      tooltip: label,
      onPressed: action,
      icon: Icon(icon, size: 14),
      style: IconButton.styleFrom(shape: const RoundedRectangleBorder()),
    ),
  );
}

class _StudioSidebar extends ConsumerWidget {
  const _StudioSidebar();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final view = ref.watch(studioViewProvider);
    return Container(
      width: 76,

      decoration: const BoxDecoration(
        color: AppColors.sidebar,
        border: Border(right: BorderSide(color: Color(0xFF1A2531))),
      ),
      child: Column(
        children: [
          const SizedBox(height: 30),
          _NavButton(
            icon: Icons.home_outlined,
            label: 'Inicio',
            selected: view == StudioView.home,
            onTap:
                () =>
                    ref.read(studioViewProvider.notifier).state =
                        StudioView.home,
          ),
          const SizedBox(height: 12),
          _NavButton(
            icon: Icons.auto_awesome,
            label: 'Atomizador',
            selected: view == StudioView.atomizer || view == StudioView.mixer,
            onTap:
                () =>
                    ref.read(studioViewProvider.notifier).state =
                        StudioView.atomizer,
          ),
          const Spacer(),
          _NavButton(
            icon: Icons.settings_outlined,
            label: 'Ajustes',
            selected: view == StudioView.settings,
            onTap:
                () =>
                    ref.read(studioViewProvider.notifier).state =
                        StudioView.settings,
          ),
          const SizedBox(height: 10),
          IconButton(
            tooltip: 'Ayuda',
            onPressed: () => _showStudioHelp(context),
            icon: const Icon(Icons.help_outline, size: 17),
          ),
          const SizedBox(height: 14),
        ],
      ),
    );
  }
}

class _NavButton extends StatelessWidget {
  const _NavButton({
    required this.icon,
    required this.label,
    required this.selected,
    required this.onTap,
  });
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 10),
    child: Tooltip(
      message: label,
      child: Semantics(
        selected: selected,
        button: true,
        child: Material(
          color: selected ? const Color(0xFF0F2A28) : Colors.transparent,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(13),
            side: BorderSide(
              color:
                  selected
                      ? AppColors.seaGlass.withValues(alpha: .24)
                      : Colors.transparent,
            ),
          ),
          child: InkWell(
            onTap: onTap,
            borderRadius: BorderRadius.circular(13),
            child: SizedBox(
              width: 56,
              height: 64,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(
                    icon,
                    size: 20,
                    color: selected ? AppColors.seaGlass : AppColors.muted,
                  ),
                  const SizedBox(height: 6),
                  Text(
                    label,
                    maxLines: 1,
                    style: TextStyle(
                      fontSize: 8.2,
                      color:
                          selected ? const Color(0xFF7FE9D9) : AppColors.muted,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

void _showStudioHelp(BuildContext context) => showDialog<void>(
  context: context,
  builder:
      (context) => AlertDialog(
        title: const Text('Formatos y límites'),
        content: const SizedBox(
          width: 470,
          child: Text(
            'Importa WAV, MP3, FLAC o M4A. También puedes arrastrar archivos o copiarlos en el Explorador y pegarlos con Ctrl+V.\n\nLa separación genera cinco stems en este equipo. Los modelos se descargan una sola vez con tu autorización. Conserva el audio original para reabrir el proyecto.',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Entendido'),
          ),
        ],
      ),
);
