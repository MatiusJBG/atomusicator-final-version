import 'package:atomusicator/core/di/workbench_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('an import selects its first source and keeps previous sources', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final controller = container.read(workbenchProvider.notifier);
    const old = SelectedAudioFile(
      path: '/old.wav',
      name: 'old.wav',
      sizeBytes: 1,
    );
    const flac = SelectedAudioFile(
      path: '/new.flac',
      name: 'new.flac',
      sizeBytes: 2,
    );
    controller.addSources([old]);
    controller.addSources([flac]);
    expect(container.read(workbenchProvider).selectedSource, flac);
    expect(container.read(workbenchProvider).sources, [old, flac]);
    controller.addSources([flac]);
    expect(container.read(workbenchProvider).sources, [old, flac]);
  });
}
