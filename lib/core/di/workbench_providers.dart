import 'package:flutter_riverpod/flutter_riverpod.dart';

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
    state = WorkbenchState(
      sources: List.unmodifiable([...state.sources, ...sources]),
    );
  }
}

final workbenchProvider = NotifierProvider<WorkbenchController, WorkbenchState>(
  WorkbenchController.new,
);
