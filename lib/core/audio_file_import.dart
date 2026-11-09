import 'dart:io';

import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

/// The same input formats are offered by the picker and accepted by drop/paste.
abstract final class AudioFileImport {
  static const extensions = ['wav', 'mp3', 'flac', 'm4a'];
  static const _channel = MethodChannel('atomusicator/audio_import');

  static bool accepts(String name) =>
      extensions.contains(name.split('.').last.toLowerCase());

  static Future<List<String>> clipboardPaths() async =>
      await _channel.invokeListMethod<String>('readFilePaths') ?? const [];

  /// Copies a picker file into application support before the engine opens it.
  static Future<({String path, String name, int sizeBytes})?> materialize({
    required String name,
    String? sourcePath,
    List<int>? bytes,
    int sizeBytes = 0,
  }) async {
    if (!accepts(name)) return null;
    if (!Platform.isAndroid) {
      if (sourcePath == null || sourcePath.isEmpty) return null;
      return (path: sourcePath, name: name, sizeBytes: sizeBytes);
    }
    final support = await getApplicationSupportDirectory();
    final directory = Directory(
      '${support.path}/imports/${DateTime.now().microsecondsSinceEpoch}',
    );
    await directory.create(recursive: true);
    final safe = name.replaceAll(RegExp(r'[\\/:*?"<>|]'), '_');
    final target = File('${directory.path}/$safe');
    if (sourcePath != null && sourcePath.isNotEmpty) {
      await File(sourcePath).copy(target.path);
    } else if (bytes != null) {
      await target.writeAsBytes(bytes, flush: true);
    } else {
      return null;
    }
    return (path: target.path, name: name, sizeBytes: await target.length());
  }
}
