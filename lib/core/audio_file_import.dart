import 'package:flutter/services.dart';

/// The same input formats are offered by the picker and accepted by drop/paste.
abstract final class AudioFileImport {
  static const extensions = ['wav', 'mp3', 'flac', 'm4a'];
  static const _channel = MethodChannel('atomusicator/audio_import');

  static bool accepts(String name) =>
      extensions.contains(name.split('.').last.toLowerCase());

  static Future<List<String>> clipboardPaths() async =>
      await _channel.invokeListMethod<String>('readFilePaths') ?? const [];
}
