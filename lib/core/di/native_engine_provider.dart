import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import '../../src/rust_bridge/api.dart';
import '../../src/rust_bridge/frb_generated.dart';

class NativeEngineStatus {
  const NativeEngineStatus({
    required this.engineVersion,
    required this.applicationVersion,
    required this.supportedStems,
  });

  final String engineVersion;
  final String applicationVersion;
  final List<String> supportedStems;
}

final nativeEngineStatusProvider = FutureProvider<NativeEngineStatus>((
  ref,
) async {
  await _initializeRustBridge();
  final results = await Future.wait([
    getEngineVersion(),
    applicationVersion(),
    supportedStems(),
  ]);
  return NativeEngineStatus(
    engineVersion: results[0] as String,
    applicationVersion: results[1] as String,
    supportedStems: results[2] as List<String>,
  );
});

Future<void>? _initialization;

Future<void> _initializeRustBridge() => _initialization ??= _initialize();

Future<void> _initialize() async {
  if (RustLib.instance.initialized) return;
  final executableDirectory = File(Platform.resolvedExecutable).parent.path;
  final libraryName = switch (Platform.operatingSystem) {
    'windows' => 'atomusicator_bootstrap_ffi.dll',
    'macos' => 'libatomusicator_bootstrap_ffi.dylib',
    _ => 'libatomusicator_bootstrap_ffi.so',
  };
  final libraryPath =
      '$executableDirectory${Platform.pathSeparator}$libraryName';
  await RustLib.init(externalLibrary: ExternalLibrary.open(libraryPath));
}
