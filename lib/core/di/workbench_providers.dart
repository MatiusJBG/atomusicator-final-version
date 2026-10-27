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
  const WorkbenchState({this.sources = const []});

  final List<SelectedAudioFile> sources;
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
    );
  }

  void removeSource(String path) {
    state = WorkbenchState(
      sources: List.unmodifiable(
        state.sources.where((file) => file.path != path),
      ),
    );
  }

  String _pathKey(String path) {
    final normalized = path.replaceAll('\\', '/');
    return Platform.isWindows ? normalized.toLowerCase() : normalized;
  }
}

final workbenchProvider = NotifierProvider<WorkbenchController, WorkbenchState>(
  WorkbenchController.new,
);
