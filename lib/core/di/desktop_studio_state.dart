import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../../src/rust_bridge/api.dart' as native;
import 'native_engine_provider.dart';
import 'studio_controller.dart';
import 'workbench_providers.dart';

enum StudioView { home, atomizer, mixer, settings }

final studioViewProvider = StateProvider<StudioView>((ref) {
  if (ref.read(studioProvider).session != null) return StudioView.mixer;
  return ref.read(workbenchProvider).hasSources
      ? StudioView.atomizer
      : StudioView.home;
});
final separationFlowProvider = StateProvider<bool>((ref) => false);
final separationResultProvider = StateProvider<bool>((ref) => false);
final selectedStemTypesProvider = StateProvider<Set<String>>(
  (ref) => {'lead_vocal', 'backing_vocals', 'drums', 'bass', 'instrumental'},
);
final studioTextScaleProvider = StateProvider<double>((ref) => 1);
final studioGridProvider = StateProvider<bool>((ref) => true);
final studioReducedMotionProvider = StateProvider<bool>((ref) => true);
final studioDensityProvider = StateProvider<double>((ref) => 1);
final favoriteProjectsProvider = StateProvider<Set<String>>((ref) => {});
final desktopPreferencesFileProvider = Provider<File>((ref) {
  final base = Platform.environment['LOCALAPPDATA'] ?? Directory.current.path;
  return File('$base/Atomusicator/desktop-ui.json');
});
final desktopPreferencesLoadProvider = FutureProvider<void>((ref) async {
  final file = ref.read(desktopPreferencesFileProvider);
  if (!await file.exists()) return;
  final values = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
  ref
      .read(studioTextScaleProvider.notifier)
      .state = ((values['textScale'] as num?)?.toDouble() ?? 1).clamp(.9, 1.4);
  ref
      .read(studioDensityProvider.notifier)
      .state = ((values['density'] as num?)?.toDouble() ?? 1).clamp(.9, 1.1);
  ref.read(studioGridProvider.notifier).state = values['grid'] as bool? ?? true;
  ref.read(studioReducedMotionProvider.notifier).state =
      values['reducedMotion'] as bool? ?? true;
  ref.read(favoriteProjectsProvider.notifier).state =
      (values['favorites'] as List?)?.whereType<String>().toSet() ?? {};
});

Future<void> saveDesktopPreferences(WidgetRef ref) async {
  final file = ref.read(desktopPreferencesFileProvider);
  final data = jsonEncode({
    'textScale': ref.read(studioTextScaleProvider),
    'density': ref.read(studioDensityProvider),
    'grid': ref.read(studioGridProvider),
    'reducedMotion': ref.read(studioReducedMotionProvider),
    'favorites': ref.read(favoriteProjectsProvider).toList(),
  });
  await file.parent.create(recursive: true);
  await file.writeAsString(data, flush: true);
}

class StudioProject {
  const StudioProject({
    required this.directory,
    required this.name,
    required this.duration,
    required this.sampleRate,
    required this.channels,
    required this.stems,
    required this.modified,
  });
  final String directory, name;
  final double duration;
  final int sampleRate, channels, stems;
  final DateTime modified;
}

final projectsDirectoryProvider = FutureProvider<String>((ref) async {
  await ref.watch(nativeEngineStatusProvider.future);
  return native.projectsDirectory();
});
final studioProjectsProvider = FutureProvider<List<StudioProject>>((ref) async {
  final root = Directory(await ref.watch(projectsDirectoryProvider.future));
  if (!await root.exists()) return [];
  final projects = <StudioProject>[];
  // Session folders live immediately under the configured projects directory.
  // Do not crawl the user's disk or follow directory links.
  await for (final entity in root.list(followLinks: false)) {
    if (entity is! Directory) continue;
    final file = File('${entity.path}/session.json');
    if (!await file.exists()) continue;
    try {
      final data =
          jsonDecode(await file.readAsString()) as Map<String, dynamic>;
      final separation = data['separation'] as Map<String, dynamic>?;
      if (separation?['status'] != 'complete') continue;
      projects.add(
        StudioProject(
          directory: entity.path,
          name:
              data['file_name'] as String? ??
              entity.uri.pathSegments.where((s) => s.isNotEmpty).last,
          duration: (data['duration_seconds'] as num?)?.toDouble() ?? 0,
          sampleRate: (data['sample_rate_hz'] as num?)?.toInt() ?? 0,
          channels: (data['channels'] as num?)?.toInt() ?? 0,
          stems: (separation?['stems'] as List?)?.length ?? 0,
          modified: (await file.stat()).modified,
        ),
      );
    } on FormatException {
      continue;
    } on FileSystemException {
      continue;
    } on TypeError {
      continue;
    }
  }
  projects.sort((a, b) => b.modified.compareTo(a.modified));
  return projects;
});

class SourceAudioPreview {
  const SourceAudioPreview({
    required this.duration,
    required this.sampleRate,
    required this.channels,
    required this.peaks,
    this.bitDepth,
  });
  final double duration;
  final int sampleRate, channels;
  final int? bitDepth;
  final List<double> peaks;
}

final previewAudioToolsDirectoryProvider = Provider<String>(
  (ref) => '${File(Platform.resolvedExecutable).parent.path}/audio-tools/bin',
);

final sourceAudioPreviewProvider = FutureProvider.autoDispose.family<
  SourceAudioPreview?,
  String
>((ref, path) async {
  // Native audio tools are already shipped beside the application. Decoding a
  // source preview does not load it as a mixer session or start playback.
  if (!await File(path).exists()) return null;
  final tools = ref.watch(previewAudioToolsDirectoryProvider);
  final suffix = Platform.isWindows ? '.exe' : '';
  final probePath = '$tools/ffprobe$suffix';
  final decoderPath = '$tools/ffmpeg$suffix';
  if (!await File(probePath).exists() || !await File(decoderPath).exists()) {
    return null;
  }
  Process? process;
  var disposed = false;
  ref.onDispose(() {
    disposed = true;
    process?.kill();
  });
  final probe = await Process.run(probePath, [
    '-v',
    'error',
    '-select_streams',
    'a:0',
    '-show_entries',
    'stream=sample_rate,channels,bits_per_raw_sample,bits_per_sample:format=duration',
    '-of',
    'json',
    path,
  ]);
  if (probe.exitCode != 0) {
    throw const FormatException('No se pudo leer el audio.');
  }
  final metadata = jsonDecode(probe.stdout as String) as Map<String, dynamic>;
  final stream = (metadata['streams'] as List).first as Map<String, dynamic>;
  final duration = double.tryParse('${metadata['format']['duration']}') ?? 0;
  if (duration <= 0 || !duration.isFinite || disposed) return null;
  const buckets = 650;
  final peaks = List<double>.filled(buckets * 2, 0);
  final expectedSamples = math.max(1.0, duration * 4000);
  process = await Process.start(decoderPath, [
    '-v',
    'error',
    '-nostdin',
    '-i',
    path,
    '-map',
    '0:a:0',
    '-ac',
    '1',
    '-ar',
    '4000',
    '-f',
    'f32le',
    'pipe:1',
  ]);
  if (disposed) process.kill();
  final stderrDone = process.stderr.drain<void>();
  var sample = 0;
  var tail = <int>[];
  await for (final chunk in process.stdout) {
    final bytes = Uint8List.fromList([...tail, ...chunk]);
    final data = ByteData.sublistView(bytes);
    final limit = bytes.length - bytes.length % 4;
    for (var offset = 0; offset < limit; offset += 4) {
      final value = data.getFloat32(offset, Endian.little);
      final index =
          math.min(
            buckets - 1,
            (sample++ * buckets / expectedSamples).floor(),
          ) *
          2;
      if (value.isFinite) {
        peaks[index] = math.min(peaks[index], value);
        peaks[index + 1] = math.max(peaks[index + 1], value);
      }
    }
    tail = bytes.sublist(limit);
  }
  final exitCode = await process.exitCode;
  await stderrDone;
  if (disposed) return null;
  if (exitCode != 0) {
    throw const FormatException('No se pudo generar la forma de onda.');
  }
  final rawBits = int.tryParse('${stream['bits_per_raw_sample']}') ?? 0;
  final bits =
      rawBits > 0 ? rawBits : int.tryParse('${stream['bits_per_sample']}');
  return SourceAudioPreview(
    duration: duration,
    sampleRate: int.tryParse('${stream['sample_rate']}') ?? 0,
    channels: (stream['channels'] as num?)?.toInt() ?? 0,
    bitDepth: bits == 0 ? null : bits,
    peaks: peaks,
  );
});
