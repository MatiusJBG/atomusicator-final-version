part of 'audio_workbench.dart';

class _DesktopTitleBar extends ConsumerWidget {
  const _DesktopTitleBar();
  @override
  Widget build(BuildContext context, WidgetRef ref) => SizedBox(
    height: 38,
    child: Row(
      children: [
        Expanded(
          child: desktopDragArea(
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
        if (isDesktopHost) ...[
          _WindowButton(
            icon: Icons.remove,
            label: 'Minimizar',
            action: () {
              minimizeApplicationWindow();
            },
          ),
          _WindowButton(
            icon: Icons.crop_square,
            label: 'Maximizar',
            action: () {
              toggleMaximizeApplicationWindow();
            },
          ),
          _WindowButton(
            icon: Icons.close,
            label: 'Cerrar',
            action: () {
              closeApplicationWindow();
            },
          ),
        ],
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
    final hasSession = ref.watch(
      studioProvider.select((s) => s.session != null),
    );
    return Container(
      width: 76,
      decoration: const BoxDecoration(
        color: AppColors.sidebar,
        border: Border(right: BorderSide(color: Color(0xFF1A2531))),
      ),
      child: LayoutBuilder(
        builder: (context, box) {
          final compact = box.maxHeight < 420;
          final gap = compact ? 6.0 : 12.0;
          return Column(
            children: [
              SizedBox(height: compact ? 8 : 30),
              _NavButton(
                icon: Icons.home_outlined,
                label: 'Inicio',
                selected: view == StudioView.home,
                compact: compact,
                onTap:
                    () =>
                        ref.read(studioViewProvider.notifier).state =
                            StudioView.home,
              ),
              SizedBox(height: gap),
              _NavButton(
                icon: Icons.auto_awesome,
                label: 'Atomizador',
                selected: view == StudioView.atomizer,
                compact: compact,
                onTap:
                    () =>
                        ref.read(studioViewProvider.notifier).state =
                            StudioView.atomizer,
              ),
              SizedBox(height: gap),
              _NavButton(
                icon: Icons.tune,
                label: 'Mezclador',
                selected: view == StudioView.mixer,
                compact: compact,
                onTap: () {
                  if (!hasSession) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(
                        content: Text(
                          'Separa un audio para abrir el mezclador.',
                        ),
                      ),
                    );
                    return;
                  }
                  ref.read(studioViewProvider.notifier).state =
                      StudioView.mixer;
                },
              ),
              const Spacer(),
              _NavButton(
                icon: Icons.settings_outlined,
                label: 'Ajustes',
                selected: view == StudioView.settings,
                compact: compact,
                onTap:
                    () =>
                        ref.read(studioViewProvider.notifier).state =
                            StudioView.settings,
              ),
              SizedBox(height: compact ? 4 : 10),
              IconButton(
                tooltip: 'Ayuda',
                visualDensity:
                    compact ? VisualDensity.compact : VisualDensity.standard,
                onPressed: () => _showStudioHelp(context),
                icon: const Icon(Icons.help_outline, size: 17),
              ),
              SizedBox(height: compact ? 6 : 14),
            ],
          );
        },
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
    this.compact = false,
  });
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;
  final bool compact;
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
              height: compact ? 48 : 64,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(
                    icon,
                    size: compact ? 18 : 20,
                    color: selected ? AppColors.seaGlass : AppColors.muted,
                  ),
                  SizedBox(height: compact ? 3 : 6),
                  Text(
                    label,
                    maxLines: 1,
                    style: TextStyle(
                      fontSize: compact ? 7.8 : 8.2,
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
        content: SizedBox(
          width: 470,
          child: Text(
            isDesktopHost
                ? 'Importa WAV, MP3, FLAC o M4A. También puedes arrastrar archivos o copiarlos en el Explorador y pegarlos con Ctrl+V.\n\nLa separación genera cinco stems en este equipo. Los modelos se descargan una sola vez con tu autorización. Conserva el audio original para reabrir el proyecto.'
                : 'Importa WAV, MP3, FLAC o M4A con el selector o compartiendo un audio hacia Atomusicator.\n\nLa separación genera cinco stems en este equipo. Los modelos se descargan una sola vez con tu autorización. Conserva el audio original para reabrir el proyecto.',
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
