import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'dart:io';

class SelectedAudioFile {
  const SelectedAudioFile({
    required this.path,
    required this.name,
    required this.sizeBytes,
  });

  final String path;
  final String name;
  final int sizeBytes;
}

class WorkbenchState {
  const WorkbenchState({this.sources = const [], this.selectedPath});

  final List<SelectedAudioFile> sources;
  final String? selectedPath;
  SelectedAudioFile? get selectedSource =>
      sources.where((s) => s.path == selectedPath).firstOrNull ??
      sources.firstOrNull;
  bool get hasSources => sources.isNotEmpty;
}

class WorkbenchController extends Notifier<WorkbenchState> {
  @override
  WorkbenchState build() => const WorkbenchState();

  void addSources(List<SelectedAudioFile> sources) {
    if (sources.isEmpty) return;
    final knownPaths = state.sources.map((file) => _pathKey(file.path)).toSet();
    final added = sources.where((file) => knownPaths.add(_pathKey(file.path)));
    state = WorkbenchState(
      sources: List.unmodifiable([...state.sources, ...added]),
      selectedPath: sources.first.path,
    );
  }

  void removeSource(String path) {
    state = WorkbenchState(
      sources: List.unmodifiable(
        state.sources.where((file) => file.path != path),
      ),
      selectedPath: state.selectedPath,
    );
  }

  void selectSource(String path) {
    if (state.sources.any((source) => source.path == path)) {
      state = WorkbenchState(sources: state.sources, selectedPath: path);
    }
  }

  void clearSources() => state = const WorkbenchState();

  String _pathKey(String path) {
    final normalized = path.replaceAll('\\', '/');
    return Platform.isWindows ? normalized.toLowerCase() : normalized;
  }
}

final workbenchProvider = NotifierProvider<WorkbenchController, WorkbenchState>(
  WorkbenchController.new,
);
