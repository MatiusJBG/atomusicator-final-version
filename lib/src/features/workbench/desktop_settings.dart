part of 'audio_workbench.dart';

class _StudioSettings extends ConsumerWidget {
  const _StudioSettings();
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scale = ref.watch(studioTextScaleProvider);
    final density = ref.watch(studioDensityProvider);
    final reduced = ref.watch(studioReducedMotionProvider);
    final grid = ref.watch(studioGridProvider);
    final status = ref.watch(nativeEngineStatusProvider);
    final directory = ref.watch(projectsDirectoryProvider);
    return _PageViewport(
      minHeight: MediaQuery.sizeOf(context).width < 1150 ? 1420 : 720,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Configuración y personalización',
            style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 5),
          const Text(
            'Dos grupos de ajustes: la apariencia de la interfaz y el motor de audio e IA',
            style: TextStyle(fontSize: 11, color: AppColors.muted),
          ),
          const SizedBox(height: 24),
          Expanded(
            child: Flex(
              direction:
                  MediaQuery.sizeOf(context).width < 1150
                      ? Axis.vertical
                      : Axis.horizontal,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: _Panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const _SettingsHeading(
                          icon: Icons.palette_outlined,
                          title: 'Apariencia',
                          description: 'Interfaz, contraste y paleta de ondas',
                        ),
                        const SizedBox(height: 24),
                        const _SectionLabel('TEMA DE LA INTERFAZ'),
                        const SizedBox(height: 12),
                        SizedBox(
                          height:
                              130 * MediaQuery.textScalerOf(context).scale(1),
                          child: const Row(
                            children: [
                              Expanded(
                                child: _ThemePreview(
                                  title: 'Oscuro Estudio',
                                  detail: 'Contraste AA · acento teal',
                                  selected: true,
                                ),
                              ),
                              SizedBox(width: 12),
                              Expanded(
                                child: Tooltip(
                                  message:
                                      'El tema claro no está disponible en esta versión del estudio.',
                                  child: _ThemePreview(
                                    title: 'Claro Alto Contraste',
                                    detail: 'Tema no disponible',
                                    light: true,
                                  ),
                                ),
                              ),
                              SizedBox(width: 12),
                              Expanded(
                                child: _ThemePreview(
                                  title: 'Paleta de ondas',
                                  detail: '5 stems · colores por pista',
                                  waves: true,
                                ),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(height: 24),
                        const _SectionLabel('DENSIDAD'),
                        const SizedBox(height: 10),
                        Wrap(
                          spacing: 6,
                          children: [
                            for (final item in [
                              (0.9, 'Compacta'),
                              (1.0, 'Normal'),
                              (1.1, 'Cómoda'),
                            ])
                              ChoiceChip(
                                showCheckmark: false,
                                selected: density == item.$1,
                                label: Text(
                                  item.$2,
                                  style: const TextStyle(fontSize: 10),
                                ),
                                onSelected:
                                    (_) =>
                                        ref
                                            .read(
                                              studioDensityProvider.notifier,
                                            )
                                            .state = item.$1,
                              ),
                          ],
                        ),
                        const SizedBox(height: 18),
                        const _SectionLabel('TAMAÑO DE TEXTO'),
                        Row(
                          children: [
                            const Text(
                              '90%',
                              style: TextStyle(
                                fontSize: 9,
                                color: AppColors.muted,
                              ),
                            ),
                            Expanded(
                              child: Slider(
                                value: scale,
                                min: .9,
                                max: 1.4,
                                divisions: 10,
                                semanticFormatterCallback:
                                    (v) => '${(v * 100).round()}%',
                                onChanged:
                                    (v) =>
                                        ref
                                            .read(
                                              studioTextScaleProvider.notifier,
                                            )
                                            .state = v,
                              ),
                            ),
                            _Tag('${(scale * 100).round()}%'),
                            const SizedBox(width: 8),
                            const Text(
                              '140%',
                              style: TextStyle(
                                fontSize: 9,
                                color: AppColors.muted,
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 10),
                        SwitchListTile(
                          contentPadding: EdgeInsets.zero,
                          dense: true,
                          title: const Text(
                            'Reducir movimiento',
                            style: TextStyle(fontSize: 11),
                          ),
                          subtitle: const Text(
                            'Desactiva transiciones y glow de los botones',
                            style: TextStyle(
                              fontSize: 9,
                              color: AppColors.muted,
                            ),
                          ),
                          value: reduced,
                          onChanged:
                              (v) =>
                                  ref
                                      .read(
                                        studioReducedMotionProvider.notifier,
                                      )
                                      .state = v,
                        ),
                        SwitchListTile(
                          contentPadding: EdgeInsets.zero,
                          dense: true,
                          title: const Text(
                            'Mostrar rejilla y reglas',
                            style: TextStyle(fontSize: 11),
                          ),
                          subtitle: const Text(
                            'Guías visibles en la línea de tiempo',
                            style: TextStyle(
                              fontSize: 9,
                              color: AppColors.muted,
                            ),
                          ),
                          value: grid,
                          onChanged:
                              (v) =>
                                  ref.read(studioGridProvider.notifier).state =
                                      v,
                        ),
                        const Spacer(),
                        const Text(
                          'Los cambios se aplican al instante en esta sesión',
                          style: TextStyle(
                            fontSize: 10,
                            color: AppColors.muted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                const SizedBox(width: 24, height: 24),
                Expanded(
                  child: _Panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const _SettingsHeading(
                          icon: Icons.memory_outlined,
                          title: 'Motor e IA',
                          description:
                              'Audio, aceleración por hardware y modelo de separación',
                        ),
                        const SizedBox(height: 24),
                        const _SectionLabel('MOTOR DE AUDIO'),
                        const SizedBox(height: 12),
                        const Row(
                          children: [
                            _Tag('WASAPI', color: AppColors.seaGlass),
                            SizedBox(width: 12),
                            _Tag('Windows Audio'),
                          ],
                        ),
                        const SizedBox(height: 10),
                        const Text(
                          'Salida nativa · reproducción sincronizada de los stems',
                          style: TextStyle(
                            fontSize: 10,
                            color: AppColors.muted,
                          ),
                        ),
                        const SizedBox(height: 28),
                        const _SectionLabel('ACELERACIÓN'),
                        const SizedBox(height: 12),
                        const Row(
                          children: [
                            _Tag('CPU', color: AppColors.seaGlass),
                            SizedBox(width: 12),
                            Tooltip(
                              message: 'GPU no disponible en el motor actual.',
                              child: _Tag('CUDA GPU · no disponible'),
                            ),
                          ],
                        ),
                        const SizedBox(height: 10),
                        const Text(
                          'Procesamiento en este equipo · ONNX Runtime',
                          style: TextStyle(
                            fontSize: 10,
                            color: AppColors.muted,
                          ),
                        ),
                        const SizedBox(height: 28),
                        const _SectionLabel('MODELO DE SEPARACIÓN'),
                        const SizedBox(height: 12),
                        const _Panel(
                          padding: EdgeInsets.all(14),
                          radius: 8,
                          color: AppColors.canvas,
                          child: Text(
                            'UVR / Kuielab · MDX · cinco stems',
                            style: TextStyle(fontSize: 11),
                          ),
                        ),
                        const SizedBox(height: 10),
                        const Text(
                          'Cuatro modelos verificados. Descarga inicial con autorización.',
                          style: TextStyle(
                            fontSize: 10,
                            color: AppColors.muted,
                          ),
                        ),
                        const SizedBox(height: 28),
                        const _SectionLabel('CARPETA DE PROYECTOS'),
                        const SizedBox(height: 12),
                        _Panel(
                          padding: const EdgeInsets.fromLTRB(12, 6, 6, 6),
                          radius: 8,
                          color: AppColors.canvas,
                          child: Row(
                            children: [
                              const Icon(Icons.folder_open_outlined, size: 17),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  directory.valueOrNull ??
                                      'Selecciona una carpeta',
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    fontSize: 9,
                                    color: AppColors.muted,
                                  ),
                                ),
                              ),
                              TextButton(
                                onPressed:
                                    status.hasValue
                                        ? () async {
                                          await chooseProjectsDirectory(
                                            context,
                                            ref,
                                          );
                                          ref.invalidate(
                                            projectsDirectoryProvider,
                                          );
                                        }
                                        : null,
                                child: const Text(
                                  'Cambiar',
                                  style: TextStyle(fontSize: 10),
                                ),
                              ),
                            ],
                          ),
                        ),
                        const Spacer(),
                        const _Panel(
                          color: Color(0xFF102D29),
                          radius: 8,
                          padding: EdgeInsets.all(14),
                          child: Row(
                            children: [
                              Icon(
                                Icons.shield_outlined,
                                size: 20,
                                color: AppColors.seaGlass,
                              ),
                              SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      'Procesado 100 % local',
                                      style: TextStyle(
                                        fontSize: 11,
                                        fontWeight: FontWeight.w600,
                                        color: AppColors.seaGlass,
                                      ),
                                    ),
                                    SizedBox(height: 6),
                                    Text(
                                      'El audio nunca sale del equipo.',
                                      style: TextStyle(
                                        fontSize: 10,
                                        color: AppColors.muted,
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
              ],
            ),
          ),
          const SizedBox(height: 20),
          Align(
            alignment: Alignment.centerRight,
            child: Text(
              status.hasValue
                  ? '● Motor conectado · ajustes de esta sesión'
                  : 'Motor no disponible',
              style: const TextStyle(fontSize: 10, color: AppColors.seaGlass),
            ),
          ),
        ],
      ),
    );
  }
}

class _SettingsHeading extends StatelessWidget {
  const _SettingsHeading({
    required this.icon,
    required this.title,
    required this.description,
  });
  final IconData icon;
  final String title, description;
  @override
  Widget build(BuildContext context) => Row(
    children: [
      CircleAvatar(
        radius: 20,
        backgroundColor: AppColors.seaGlass.withValues(alpha: .1),
        child: Icon(icon, size: 20, color: AppColors.seaGlass),
      ),
      const SizedBox(width: 12),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              title,
              style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 5),
            Text(
              description,
              style: const TextStyle(fontSize: 9, color: AppColors.muted),
            ),
          ],
        ),
      ),
    ],
  );
}

class _ThemePreview extends StatelessWidget {
  const _ThemePreview({
    required this.title,
    required this.detail,
    this.selected = false,
    this.light = false,
    this.waves = false,
  });
  final String title, detail;
  final bool selected, light, waves;
  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(9),
    decoration: BoxDecoration(
      color: AppColors.canvas,
      borderRadius: BorderRadius.circular(10),
      border: Border.all(color: selected ? AppColors.seaGlass : AppColors.line),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: Container(
            width: double.infinity,
            padding: const EdgeInsets.all(8),
            color: light ? const Color(0xFFE8EDF3) : const Color(0xFF111923),
            child:
                waves
                    ? Column(
                      mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                      children: [
                        for (final kind in stemNames.keys)
                          Container(height: 3, color: stemColor(kind)),
                      ],
                    )
                    : Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Align(
                          alignment: Alignment.centerRight,
                          child: Container(
                            height: 6,
                            width: 40,
                            color: AppColors.seaGlass,
                          ),
                        ),
                        const SizedBox(height: 5),
                        Container(
                          height: 4,
                          width: 45,
                          color: AppColors.muted.withValues(alpha: .4),
                        ),
                        const SizedBox(height: 5),
                        Container(
                          height: 12,
                          color: light ? Colors.white : AppColors.canvas,
                        ),
                      ],
                    ),
          ),
        ),
        const SizedBox(height: 10),
        Text(
          title,
          maxLines: 2,
          style: const TextStyle(fontSize: 10, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 5),
        Text(
          detail,
          style: const TextStyle(fontSize: 8, color: AppColors.muted),
        ),
      ],
    ),
  );
}
