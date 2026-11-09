import 'dart:io';
import 'package:flutter/services.dart';
import '../../../core/di/desktop_studio_state.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/audio_file_import.dart';
import '../../../core/di/workbench_providers.dart';
import '../../../core/di/studio_controller.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/platform/platform_audio_host.dart';
import '../../../core/platform/desktop_window.dart';
import '../../../core/platform/host_platform.dart';
import '../../../core/theme/app_theme.dart';
import 'studio_panels.dart';
import 'particle_loading_widgets.dart';
import 'compute_settings_panel.dart';
import 'execution_settings_panel.dart';
import 'cache_settings_panel.dart';

part 'desktop_sidebar.dart';
part 'desktop_sources.dart';
part 'desktop_home.dart';
part 'desktop_settings.dart';
part 'desktop_processing.dart';
part 'desktop_workspace.dart';

class AudioWorkbench extends ConsumerStatefulWidget {
  const AudioWorkbench({super.key});
  @override
  ConsumerState<AudioWorkbench> createState() => _AudioWorkbenchState();
}

class _AudioWorkbenchState extends ConsumerState<AudioWorkbench> {
  bool _dragging = false;

  @override
  void initState() {
    super.initState();
    if (Platform.isAndroid) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _consumeSharedAudio());
    }
  }

  Future<void> _consumeSharedAudio() async {
    final path = await ref.read(platformAudioHostProvider).takeSharedAudio();
    if (!mounted || path == null) return;
    final name = path.split(RegExp(r'[/\\]')).last;
    final file = File(path);
    final size = await file.exists() ? await file.length() : 0;
    if (!mounted) return;
    _addAudio([SelectedAudioFile(path: path, name: name, sizeBytes: size)]);
  }

  Future<void> _importAudio() async {
    final selection = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: AudioFileImport.extensions,
      allowMultiple: true,
      withData: false,
    );
    if (selection == null || !mounted) return;
    final total = selection.files.length;
    ref.read(fileImportStateProvider.notifier).state = (
      importing: true,
      progress: 0.08,
      label: 'Preparando importación de archivos locales…',
    );
    final materialized = <SelectedAudioFile>[];
    try {
      for (var i = 0; i < total; i++) {
        final file = selection.files[i];
        final baseProgress = i / total;
        final stepSpan = 1.0 / total;
        if (mounted) {
          ref.read(fileImportStateProvider.notifier).state = (
            importing: true,
            progress: (baseProgress + stepSpan * 0.28).clamp(0.10, 0.92),
            label: 'Cargando archivo local: ${file.name}',
          );
        }
        if (!isFlutterTestEnvironment()) {
          await Future<void>.delayed(const Duration(milliseconds: 90));
        }
        if (mounted) {
          ref.read(fileImportStateProvider.notifier).state = (
            importing: true,
            progress: (baseProgress + stepSpan * 0.62).clamp(0.18, 0.96),
            label: 'Cargando archivo local: ${file.name}',
          );
        }
        final stored = await AudioFileImport.materialize(
          name: file.name,
          sourcePath: file.path,
          bytes: file.bytes,
          sizeBytes: file.size,
        );
        if (stored != null) {
          materialized.add(
            SelectedAudioFile(
              path: stored.path,
              name: stored.name,
              sizeBytes: stored.sizeBytes,
            ),
          );
        }
        if (mounted) {
          ref.read(fileImportStateProvider.notifier).state = (
            importing: true,
            progress: ((i + 1) / total).clamp(0.25, 1.0),
            label: 'Verificando ${file.name}…',
          );
        }
        if (!isFlutterTestEnvironment()) {
          await Future<void>.delayed(const Duration(milliseconds: 90));
        }
      }
    } finally {
      if (mounted) {
        ref.read(fileImportStateProvider.notifier).state = (
          importing: false,
          progress: 1.0,
          label: '',
        );
      }
    }
    if (!mounted) return;
    _addAudio(materialized);
  }

  void _addAudio(List<SelectedAudioFile> files) {
    final accepted =
        files.where((file) => AudioFileImport.accepts(file.name)).toList();
    if (accepted.isNotEmpty) {
      ref.read(workbenchProvider.notifier).addSources(accepted);
    }
    final rejected = files.length - accepted.length;
    if (rejected > 0 && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('$rejected archivos no compatibles omitidos.')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final workbench = _buildWorkbench(context);
    if (!isDesktopHost) return workbench;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.keyV, control: true):
            _pasteAudio,
      },
      child: Focus(
        autofocus: true,
        child: DesktopFileDrop(
          onEntered: () => setState(() => _dragging = true),
          onExited: () => setState(() => _dragging = false),
          onDone: (paths) {
            setState(() => _dragging = false);
            _addPaths(paths);
          },
          child: workbench,
        ),
      ),
    );
  }

  void _addPaths(Iterable<String> paths) => _addAudio([
    for (final path in paths)
      if (path.isNotEmpty)
        SelectedAudioFile(
          path: path,
          name: path.split(RegExp(r'[/\\]')).last,
          sizeBytes: 0,
        ),
  ]);

  Future<void> _pasteAudio() async {
    if (ModalRoute.of(context)?.isCurrent != true) return;
    try {
      final paths = await AudioFileImport.clipboardPaths();
      if (!mounted) return;
      if (paths.isEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'Copia archivos de audio en el Explorador y pégalos aquí.',
            ),
          ),
        );
      } else {
        _addPaths(paths);
      }
    } on PlatformException catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            'No se pudieron leer los archivos copiados: ${error.message ?? error.code}',
          ),
        ),
      );
    } on MissingPluginException {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'Pegar archivos requiere la aplicación de Windows actualizada. Usa Importar audio.',
          ),
        ),
      );
    }
  }

  Widget _buildWorkbench(BuildContext context) {
    final studio = ref.watch(
      studioProvider.select((s) => (busy: s.busy, session: s.session)),
    );
    final view = ref.watch(studioViewProvider);
    final flow = ref.watch(separationFlowProvider);
    ref.listen(workbenchProvider, (previous, next) {
      if (next.hasSources && previous?.selectedPath != next.selectedPath) {
        ref.read(studioViewProvider.notifier).state = StudioView.atomizer;
        ref.read(separationResultProvider.notifier).state = false;
      }
    });
    ref.listen(studioProvider.select((s) => (s.busy, s.session)), (
      previous,
      next,
    ) {
      if (ref.read(separationFlowProvider) && !next.$1) {
        ref.read(separationFlowProvider.notifier).state = false;
        if (next.$2 != null &&
            ref.read(studioProvider).error == null &&
            ref.read(studioProvider).job?.status.name == 'completed') {
          ref.read(separationResultProvider.notifier).state = true;
          ref.invalidate(studioProjectsProvider);
        }
      }
    });
    final scale = ref.watch(studioTextScaleProvider);
    final media = MediaQuery.of(context);
    final phone = isPhoneLayout(context);
    return MediaQuery(
      data: media.copyWith(
        textScaler: TextScaler.linear(media.textScaler.scale(scale)),
        disableAnimations: ref.watch(studioReducedMotionProvider),
      ),
      child: Scaffold(
        body: SafeArea(
          child: Column(
            children: [
              if (isDesktopHost) const _DesktopTitleBar(),
              _SessionHeader(onImport: _importAudio),
              Expanded(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (!phone) const _StudioSidebar(),
                    Expanded(
                      child: switch (view) {
                        StudioView.home => _ProjectsHome(
                          onImport: _importAudio,
                        ),
                        StudioView.settings => const _StudioSettings(),
                        StudioView.atomizer =>
                          flow && studio.busy
                              ? const _SeparationProgress()
                              : ref.watch(separationResultProvider) &&
                                  studio.session != null
                              ? const _SeparationResult()
                              : _AtomizerScreen(
                                onImport: _importAudio,
                                dragging: _dragging,
                              ),
                        StudioView.mixer => Column(
                          children: [
                            const Expanded(
                              child: Padding(
                                padding: EdgeInsets.fromLTRB(12, 0, 16, 0),
                                child: Column(
                                  children: [
                                    StudioFeedback(),
                                    Expanded(child: StudioTracks()),
                                  ],
                                ),
                              ),
                            ),
                            const StudioTransport(),
                          ],
                        ),
                      },
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
        bottomNavigationBar:
            phone
                ? NavigationBar(
                  selectedIndex: switch (view) {
                    StudioView.home => 0,
                    StudioView.atomizer => 1,
                    StudioView.mixer => 2,
                    StudioView.settings => 3,
                  },
                  destinations: const [
                    NavigationDestination(
                      icon: Icon(Icons.home_outlined),
                      label: 'Inicio',
                    ),
                    NavigationDestination(
                      icon: Icon(Icons.auto_awesome),
                      label: 'Atomizador',
                    ),
                    NavigationDestination(
                      icon: Icon(Icons.tune),
                      label: 'Mezclador',
                    ),
                    NavigationDestination(
                      icon: Icon(Icons.settings_outlined),
                      label: 'Ajustes',
                    ),
                  ],
                  onDestinationSelected: (index) {
                    final next = switch (index) {
                      0 => StudioView.home,
                      1 => StudioView.atomizer,
                      2 => StudioView.mixer,
                      _ => StudioView.settings,
                    };
                    if (next == StudioView.mixer && studio.session == null) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(
                          content: Text(
                            'Separa un audio para abrir el mezclador.',
                          ),
                        ),
                      );
                      return;
                    }
                    ref.read(studioViewProvider.notifier).state = next;
                  },
                )
                : null,
      ),
    );
  }
}
