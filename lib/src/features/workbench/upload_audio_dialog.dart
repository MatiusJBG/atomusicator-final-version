import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/audio_file_import.dart';
import '../../../core/di/desktop_studio_state.dart';
import '../../../core/di/workbench_providers.dart';
import '../../../core/theme/app_theme.dart';
import '../../rust_bridge/api.dart' as native;

/// Result returned when user completes the Upload Audio dialog.
class UploadAudioResult {
  const UploadAudioResult({
    required this.selectedFiles,
    required this.useCustomDestination,
    this.customDestinationDirectory,
  });

  final List<SelectedAudioFile> selectedFiles;
  final bool useCustomDestination;
  final String? customDestinationDirectory;
}

/// Displays the floating modal dialog to upload audio and select project destination directory.
Future<UploadAudioResult?> showUploadAudioDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  return showDialog<UploadAudioResult>(
    context: context,
    barrierDismissible: true,
    builder: (context) => const UploadAudioDialog(),
  );
}

class UploadAudioDialog extends ConsumerStatefulWidget {
  const UploadAudioDialog({super.key});

  @override
  ConsumerState<UploadAudioDialog> createState() => _UploadAudioDialogState();
}

class _UploadAudioDialogState extends ConsumerState<UploadAudioDialog> {
  final List<SelectedAudioFile> _selectedFiles = [];
  bool _useCustomDestination = false;
  String? _customDestinationDirectory;
  bool _isPickingFile = false;
  bool _isPickingFolder = false;

  Future<void> _pickAudioFiles() async {
    setState(() => _isPickingFile = true);
    try {
      final result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: [
          'wav',
          'flac',
          'mp3',
          'ogg',
          'm4a',
          'aac',
          'opus',
          'wma',
        ],
        allowMultiple: true,
        withData: false,
      );
      if (result != null && mounted) {
        final newFiles = <SelectedAudioFile>[];
        for (final file in result.files) {
          if (file.path != null && AudioFileImport.accepts(file.name)) {
            newFiles.add(
              SelectedAudioFile(
                path: file.path!,
                name: file.name,
                sizeBytes: file.size,
              ),
            );
          }
        }
        if (newFiles.isNotEmpty) {
          setState(() {
            _selectedFiles
              ..clear()
              ..addAll(newFiles);
          });
        }
      }
    } finally {
      if (mounted) setState(() => _isPickingFile = false);
    }
  }

  Future<void> _pickCustomFolder() async {
    setState(() => _isPickingFolder = true);
    try {
      final path = await FilePicker.platform.getDirectoryPath(
        dialogTitle: 'Seleccionar carpeta de destino para el proyecto',
      );
      if (path != null && mounted) {
        setState(() {
          _customDestinationDirectory = path;
          _useCustomDestination = true;
        });
      }
    } finally {
      if (mounted) setState(() => _isPickingFolder = false);
    }
  }

  String _formatSize(int bytes) {
    if (bytes <= 0) return '';
    if (bytes < 1024 * 1024) {
      return '${(bytes / 1024).toStringAsFixed(1)} KB';
    }
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }

  String _previewProjectFolderName(String audioName) {
    final rawStem = audioName.contains('.')
        ? audioName.substring(0, audioName.lastIndexOf('.'))
        : audioName;
    final sanitized = rawStem.replaceAll(RegExp(r'[^a-zA-Z0-9_\-\.]'), '-');
    final clean = sanitized.replaceAll(RegExp(r'-+'), '-').trim();
    final name = clean.isEmpty ? 'Audio' : clean;
    final truncated = name.length > 36 ? name.substring(0, 36) : name;
    return 'Proyecto_${truncated}_5Stems_...';
  }

  @override
  Widget build(BuildContext context) {
    final defaultProjectsDir =
        ref.watch(projectsDirectoryProvider).valueOrNull ??
        'Cargando ubicación por defecto…';

    return Dialog(
      backgroundColor: AppColors.panel,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(14),
        side: const BorderSide(color: AppColors.line),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 580, maxHeight: 650),
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Header
              Row(
                children: [
                  Container(
                    width: 36,
                    height: 36,
                    decoration: BoxDecoration(
                      color: AppColors.seaGlass.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: const Icon(
                      Icons.file_upload_outlined,
                      color: AppColors.seaGlass,
                      size: 20,
                    ),
                  ),
                  const SizedBox(width: 14),
                  const Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Subir Audio',
                          style: TextStyle(
                            fontSize: 17,
                            fontWeight: FontWeight.w700,
                            color: AppColors.text,
                          ),
                        ),
                        SizedBox(height: 2),
                        Text(
                          'Elige el archivo de audio y el destino del proyecto',
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
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
              const SizedBox(height: 20),

              Flexible(
                child: SingleChildScrollView(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      // Section 1: Audio file
                      const Text(
                        'ARCHIVO DE AUDIO',
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.8,
                          color: AppColors.muted,
                        ),
                      ),
                      const SizedBox(height: 8),

                      if (_selectedFiles.isEmpty)
                        InkWell(
                          onTap: _isPickingFile ? null : _pickAudioFiles,
                          borderRadius: BorderRadius.circular(10),
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 16,
                              vertical: 22,
                            ),
                            decoration: BoxDecoration(
                              color: AppColors.canvas,
                              borderRadius: BorderRadius.circular(10),
                              border: Border.all(
                                color: AppColors.seaGlass.withValues(alpha: 0.4),
                                style: BorderStyle.solid,
                              ),
                            ),
                            child: Column(
                              children: [
                                Icon(
                                  Icons.audio_file_rounded,
                                  color: AppColors.seaGlass,
                                  size: 32,
                                ),
                                const SizedBox(height: 8),
                                Text(
                                  _isPickingFile
                                      ? 'Abriendo explorador…'
                                      : 'Haga clic para seleccionar archivo de audio',
                                  style: const TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w600,
                                    color: AppColors.text,
                                  ),
                                ),
                                const SizedBox(height: 4),
                                const Text(
                                  'Formatos compatibles: WAV, FLAC, MP3, OGG, M4A, AAC',
                                  style: TextStyle(
                                    fontSize: 10,
                                    color: AppColors.muted,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        )
                      else
                        Container(
                          padding: const EdgeInsets.all(12),
                          decoration: BoxDecoration(
                            color: AppColors.canvas,
                            borderRadius: BorderRadius.circular(10),
                            border: Border.all(color: AppColors.line),
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              for (final file in _selectedFiles) ...[
                                Row(
                                  children: [
                                    const Icon(
                                      Icons.graphic_eq_rounded,
                                      color: AppColors.seaGlass,
                                      size: 20,
                                    ),
                                    const SizedBox(width: 10),
                                    Expanded(
                                      child: Column(
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          Text(
                                            file.name,
                                            maxLines: 1,
                                            overflow: TextOverflow.ellipsis,
                                            style: const TextStyle(
                                              fontSize: 12,
                                              fontWeight: FontWeight.w600,
                                            ),
                                          ),
                                          if (file.sizeBytes > 0)
                                            Text(
                                              _formatSize(file.sizeBytes),
                                              style: const TextStyle(
                                                fontSize: 10,
                                                color: AppColors.muted,
                                              ),
                                            ),
                                        ],
                                      ),
                                    ),
                                    TextButton.icon(
                                      icon: const Icon(
                                        Icons.change_circle_outlined,
                                        size: 14,
                                      ),
                                      label: const Text(
                                        'Cambiar',
                                        style: TextStyle(fontSize: 10),
                                      ),
                                      onPressed: _pickAudioFiles,
                                    ),
                                  ],
                                ),
                                const SizedBox(height: 6),
                                Container(
                                  padding: const EdgeInsets.symmetric(
                                    horizontal: 8,
                                    vertical: 4,
                                  ),
                                  decoration: BoxDecoration(
                                    color: AppColors.panelRaised,
                                    borderRadius: BorderRadius.circular(6),
                                  ),
                                  child: Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      const Icon(
                                        Icons.folder_special_outlined,
                                        size: 13,
                                        color: AppColors.seaGlass,
                                      ),
                                      const SizedBox(width: 6),
                                      Flexible(
                                        child: Text(
                                          'Carpeta generada: ${_previewProjectFolderName(file.name)}',
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: const TextStyle(
                                            fontSize: 9.5,
                                            color: AppColors.seaGlass,
                                          ),
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),

                      const SizedBox(height: 20),

                      // Section 2: Destination directory
                      const Text(
                        'DIRECTORIO DE GUARDADO DEL PROYECTO',
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.8,
                          color: AppColors.muted,
                        ),
                      ),
                      const SizedBox(height: 8),

                      // Option 1: Default directory
                      InkWell(
                        onTap: () =>
                            setState(() => _useCustomDestination = false),
                        borderRadius: BorderRadius.circular(8),
                        child: Container(
                          padding: const EdgeInsets.all(12),
                          decoration: BoxDecoration(
                            color: !_useCustomDestination
                                ? AppColors.panelRaised
                                : AppColors.canvas,
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(
                              color: !_useCustomDestination
                                  ? AppColors.seaGlass
                                  : AppColors.line,
                            ),
                          ),
                          child: Row(
                            children: [
                              Container(
                                width: 18,
                                height: 18,
                                decoration: BoxDecoration(
                                  shape: BoxShape.circle,
                                  border: Border.all(
                                    color: !_useCustomDestination
                                        ? AppColors.seaGlass
                                        : AppColors.muted,
                                    width: 2,
                                  ),
                                ),
                                child: !_useCustomDestination
                                    ? Center(
                                        child: Container(
                                          width: 9,
                                          height: 9,
                                          decoration: const BoxDecoration(
                                            shape: BoxShape.circle,
                                            color: AppColors.seaGlass,
                                          ),
                                        ),
                                      )
                                    : null,
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    const Text(
                                      'Directorio por defecto (Recomendado)',
                                      style: TextStyle(
                                        fontSize: 11.5,
                                        fontWeight: FontWeight.w600,
                                      ),
                                    ),
                                    const SizedBox(height: 2),
                                    Text(
                                      defaultProjectsDir,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: const TextStyle(
                                        fontSize: 9.5,
                                        color: AppColors.muted,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(height: 8),

                      // Option 2: Custom directory
                      InkWell(
                        onTap: () {
                          setState(() => _useCustomDestination = true);
                          if (_customDestinationDirectory == null) {
                            _pickCustomFolder();
                          }
                        },
                        borderRadius: BorderRadius.circular(8),
                        child: Container(
                          padding: const EdgeInsets.all(12),
                          decoration: BoxDecoration(
                            color: _useCustomDestination
                                ? AppColors.panelRaised
                                : AppColors.canvas,
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(
                              color: _useCustomDestination
                                  ? AppColors.seaGlass
                                  : AppColors.line,
                            ),
                          ),
                          child: Row(
                            children: [
                              Container(
                                width: 18,
                                height: 18,
                                decoration: BoxDecoration(
                                  shape: BoxShape.circle,
                                  border: Border.all(
                                    color: _useCustomDestination
                                        ? AppColors.seaGlass
                                        : AppColors.muted,
                                    width: 2,
                                  ),
                                ),
                                child: _useCustomDestination
                                    ? Center(
                                        child: Container(
                                          width: 9,
                                          height: 9,
                                          decoration: const BoxDecoration(
                                            shape: BoxShape.circle,
                                            color: AppColors.seaGlass,
                                          ),
                                        ),
                                      )
                                    : null,
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    const Text(
                                      'Elegir directorio personalizado',
                                      style: TextStyle(
                                        fontSize: 11.5,
                                        fontWeight: FontWeight.w600,
                                      ),
                                    ),
                                    const SizedBox(height: 2),
                                    Text(
                                      _customDestinationDirectory ??
                                          'Selecciona una carpeta donde guardar el proyecto y stems...',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                        fontSize: 9.5,
                                        color: _customDestinationDirectory !=
                                                null
                                            ? AppColors.text
                                            : AppColors.muted,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              const SizedBox(width: 8),
                              OutlinedButton.icon(
                                icon: const Icon(
                                  Icons.folder_open_rounded,
                                  size: 14,
                                ),
                                label: const Text(
                                  'Examinar…',
                                  style: TextStyle(fontSize: 10),
                                ),
                                onPressed: _isPickingFolder
                                    ? null
                                    : _pickCustomFolder,
                              ),
                            ],
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),

              const SizedBox(height: 20),

              // Bottom actions
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('Cancelar'),
                  ),
                  const SizedBox(width: 12),
                  ElevatedButton.icon(
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppColors.seaGlass,
                      foregroundColor: AppColors.canvas,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 18,
                        vertical: 12,
                      ),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(8),
                      ),
                    ),
                    icon: const Icon(Icons.arrow_forward_rounded, size: 16),
                    label: const Text(
                      'Subir y Procesar Audio',
                      style: TextStyle(
                        fontSize: 11.5,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    onPressed: _selectedFiles.isEmpty
                        ? null
                        : () async {
                            if (_useCustomDestination &&
                                _customDestinationDirectory != null) {
                              try {
                                await native.setProjectsDirectory(
                                  directory: _customDestinationDirectory!,
                                );
                                ref.invalidate(projectsDirectoryProvider);
                                ref.invalidate(studioProjectsProvider);
                              } catch (e) {
                                debugPrint(
                                  'Error setting projects directory: $e',
                                );
                              }
                            }
                            if (context.mounted) {
                              Navigator.of(context).pop(
                                UploadAudioResult(
                                  selectedFiles: _selectedFiles,
                                  useCustomDestination: _useCustomDestination,
                                  customDestinationDirectory:
                                      _customDestinationDirectory,
                                ),
                              );
                            }
                          },
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
