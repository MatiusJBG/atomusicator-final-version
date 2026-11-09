import 'dart:ffi' as ffi;
import 'dart:io';
import 'package:ffi/ffi.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import '../../src/rust_bridge/api.dart';
import '../../src/rust_bridge/frb_generated.dart';
import 'native_library.dart';

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
  final libPath = requireNativeLibrary();

  if (Platform.isWindows) {
    final nativeDir = File(libPath).parent.path;
    // 1. Add native directory to DLL search path before System32
    try {
      final kernel32 = ffi.DynamicLibrary.open('kernel32.dll');
      final setDllDirectoryW = kernel32.lookupFunction<
        ffi.Int32 Function(ffi.Pointer<Utf16>),
        int Function(ffi.Pointer<Utf16>)
      >('SetDllDirectoryW');
      final dirPtr = nativeDir.toNativeUtf16();
      try {
        setDllDirectoryW(dirPtr);
      } finally {
        calloc.free(dirPtr);
      }
    } catch (_) {}

    // 2. Pre-load bundled onnxruntime.dll so Windows never falls back to
    // C:\Windows\System32\onnxruntime.dll (which is an older incompatible v1.17 build)
    final searchDirs = [
      nativeDir,
      File(Platform.resolvedExecutable).parent.path,
      '${Directory.current.path}\\build\\windows\\x64\\runner\\Release',
      '${Directory.current.path}\\build\\windows\\x64\\runner\\Debug',
      '${Directory.current.path}\\rust_core\\target\\release',
      Directory.current.path,
    ];
    for (final dir in searchDirs) {
      final onnxFile = File('$dir\\onnxruntime.dll');
      if (onnxFile.existsSync()) {
        try {
          ffi.DynamicLibrary.open(onnxFile.path);
          break;
        } catch (_) {}
      }
    }
  }

  await RustLib.init(
    externalLibrary: ExternalLibrary.open(libPath),
  );
}
