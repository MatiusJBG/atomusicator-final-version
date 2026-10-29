import 'dart:math' as math;
import 'dart:io';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../../core/di/studio_controller.dart';
import '../../../core/di/native_engine_provider.dart';
import '../../../core/di/workbench_providers.dart';
import '../../../core/di/desktop_studio_state.dart';
import '../../../core/theme/app_theme.dart';
import '../../rust_bridge/api.dart' as native;

part 'studio_timeline.dart';
part 'studio_dock.dart';
part 'studio_export.dart';

const stemNames = {
  'lead_vocal': 'Voz principal',
  'backing_vocals': 'Coros',
  'drums': 'Batería',
  'bass': 'Bajo',
  'instrumental': 'Instrumental',
};
Color stemColor(String kind) => switch (kind) {
  'lead_vocal' => AppColors.coral,
  'backing_vocals' => AppColors.lavender,
  'drums' => AppColors.amber,
  'bass' => AppColors.blue,
  _ => AppColors.green,
};
IconData stemIcon(String kind) => switch (kind) {
  'lead_vocal' => Icons.mic_none,
  'backing_vocals' => Icons.people_outline,
  'drums' => Icons.album_outlined,
  'bass' => Icons.music_note_outlined,
  _ => Icons.piano_outlined,
};
String formatStudioTime(double value) {
  final seconds = value.isFinite ? value.floor().clamp(0, 2147483647) : 0;
  return '${(seconds ~/ 60).toString().padLeft(2, '0')}:${(seconds % 60).toString().padLeft(2, '0')}';
}

final timelineZoomProvider = StateProvider<double>((ref) => 1);

Future<void> openStudioFolder(BuildContext context, String directory) async {
  try {
    if (Platform.isWindows) {
      await Process.start('explorer.exe', [directory]);
    } else if (Platform.isMacOS) {
      await Process.start('open', [directory]);
    } else {
      await Process.start('xdg-open', [directory]);
    }
  } on ProcessException {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(directory)));
    }
  }
}

Future<void> openStudioSession(BuildContext context, WidgetRef ref) async {
  final directory = await FilePicker.platform.getDirectoryPath(
    dialogTitle: 'Abrir una carpeta de sesión con session.json',
  );
  if (directory != null) {
    await ref.read(studioProvider.notifier).open(directory);
  }
}

Future<void> chooseProjectsDirectory(
  BuildContext context,
  WidgetRef ref,
) async {
  try {
    await ref.read(nativeEngineStatusProvider.future);
    final current = await native.projectsDirectory();
    final directory = await FilePicker.platform.getDirectoryPath(
      dialogTitle: 'Carpeta de proyectos',
      initialDirectory: current,
    );
    if (directory != null) {
      await native.setProjectsDirectory(directory: directory);
    }
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('$error')));
    }
  }
}

class TrackLevelMeter extends StatelessWidget {
  const TrackLevelMeter({
    super.key,
    required this.stemType,
    required this.peak,
  });
  final String stemType;
  final double peak;
  @override
  Widget build(BuildContext context) {
    final level = peak.isFinite ? peak.clamp(0.0, 1.0) : 0.0;
    final name = stemNames[stemType] ?? stemType;
    return Tooltip(
      message:
          '$name level: ${peak > 1 ? 'clipping' : 'post fader, before master'}',
      child: LinearProgressIndicator(
        key: ValueKey('meter-$stemType'),
        value: level,
        minHeight: 4,
        color: peak > 1 ? AppColors.coral : AppColors.seaGlass,
        backgroundColor: AppColors.line,
        semanticsLabel: '$name audio level',
      ),
    );
  }
}

class StudioFeedback extends ConsumerWidget {
  const StudioFeedback({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(studioProvider);
    final job = state.job;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 140),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ref
                .watch(recoverableJobsProvider)
                .when(
                  loading: () => const SizedBox.shrink(),
                  error:
                      (error, _) =>
                          ref.watch(nativeEngineStatusProvider).hasValue
                              ? Row(
                                children: [
                                  const Expanded(
                                    child: Text(
                                      'No se pudo leer el historial de recuperación.',
                                    ),
                                  ),
                                  TextButton(
                                    onPressed:
                                        () => ref.invalidate(
                                          recoverableJobsProvider,
                                        ),
                                    child: const Text('Reintentar'),
                                  ),
                                ],
                              )
                              : const SizedBox.shrink(),
                  data:
                      (jobs) => Column(
                        children: [
                          for (final saved in jobs)
                            Row(
                              children: [
                                const Icon(Icons.restore, size: 16),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Text(
                                    'Separación interrumpida: ${saved.sourcePath.split(RegExp(r'[/\\]')).last}',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                ),
                                TextButton(
                                  onPressed:
                                      () => ref
                                          .read(studioProvider.notifier)
                                          .resumeSavedJob(saved.id),
                                  child: const Text('Reanudar'),
                                ),
                                TextButton(
                                  onPressed:
                                      () => ref
                                          .read(studioProvider.notifier)
                                          .discardSavedJob(saved.id),
                                  child: const Text('Descartar'),
                                ),
                              ],
                            ),
                        ],
                      ),
                ),
            if (state.busy) ...[
              LinearProgressIndicator(
                value: jobIsActive(job?.status) ? job!.progress : null,
              ),
              Row(
                children: [
                  Expanded(
                    child: Text(job?.message ?? 'Iniciando…', maxLines: 2),
                  ),
                  TextButton(
                    onPressed:
                        job?.status == native.UiJobStatus.running
                            ? () => ref.read(studioProvider.notifier).cancel()
                            : null,
                    child: const Text('Cancelar'),
                  ),
                ],
              ),
            ],
            if (state.error != null)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 5),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SelectableText(
                      state.error!,
                      style: const TextStyle(color: AppColors.coral),
                    ),
                    if (state.error!.contains('Authorize'))
                      TextButton(
                        onPressed: () async {
                          final sources = ref.read(workbenchProvider).sources;
                          if (sources.isEmpty) return;
                          final approved = await showDialog<bool>(
                            context: context,
                            builder:
                                (context) => AlertDialog(
                                  title: const Text(
                                    '¿Descargar los modelos de separación?',
                                  ),
                                  content: const Text(
                                    'Se descargarán los cuatro modelos UVR/Kuielab revisados (unos 180 MB). Se verifica SHA-256 antes de procesar. Los modelos se guardan en este equipo y tu audio permanece local. UVR, Anjok07, aufr33 y Kuielab/KINoAI — MIT.',
                                  ),
                                  actions: [
                                    TextButton(
                                      onPressed:
                                          () => Navigator.pop(context, false),
                                      child: const Text('Cancelar'),
                                    ),
                                    FilledButton(
                                      onPressed:
                                          () => Navigator.pop(context, true),
                                      child: const Text('Descargar y separar'),
                                    ),
                                  ],
                                ),
                          );
                          if (approved == true) {
                            await ref
                                .read(studioProvider.notifier)
                                .separate(
                                  ref
                                      .read(workbenchProvider)
                                      .selectedSource!
                                      .path,
                                  allowDownload: true,
                                );
                          }
                        },
                        child: const Text('Descargar modelos y reintentar'),
                      ),
                  ],
                ),
              ),
            if (job?.status == native.UiJobStatus.cancelled)
              const Text(
                'Operación cancelada. Se conserva la sesión anterior.',
              ),
            if (job?.status == native.UiJobStatus.completed &&
                job?.outputDirectory != null)
              SelectableText('Guardado en ${job!.outputDirectory}'),
          ],
        ),
      ),
    );
  }
}
