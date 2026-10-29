import 'dart:io';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import '../../../core/di/desktop_studio_state.dart';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/audio_file_import.dart';
import '../../../core/di/workbench_providers.dart';
import '../../../core/di/studio_controller.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/theme/app_theme.dart';
import 'studio_panels.dart';

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
  Future<void> _importAudio() async {
    final selection = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: AudioFileImport.extensions,
      allowMultiple: true,
      withData: false,
    );
    if (selection == null || !mounted) return;
    _addAudio([
      for (final file in selection.files)
        if (file.path != null)
          SelectedAudioFile(
            path: file.path!,
            name: file.name,
            sizeBytes: file.size,
          ),
    ]);
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
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.keyV, control: true):
            _pasteAudio,
      },
      child: Focus(
        autofocus: true,
        child: DropTarget(
          onDragEntered: (_) => setState(() => _dragging = true),
          onDragExited: (_) => setState(() => _dragging = false),
          onDragDone: (details) {
            setState(() => _dragging = false);
            _addPaths(details.files.map((file) => file.path));
          },
          child: _buildWorkbench(context),
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
    final studio = ref.watch(studioProvider);
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
    return MediaQuery(
      data: media.copyWith(
        textScaler: TextScaler.linear(media.textScaler.scale(scale)),
        disableAnimations: ref.watch(studioReducedMotionProvider),
      ),
      child: Scaffold(
        body: SafeArea(
          child: Column(
            children: [
              const _DesktopTitleBar(),
              _SessionHeader(onImport: _importAudio),
              Expanded(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const _StudioSidebar(),
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
      ),
    );
  }
}
