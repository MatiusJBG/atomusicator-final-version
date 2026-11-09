import 'dart:io';

/// Resolves `atomusicator_bootstrap_ffi` without hardcoded developer paths.
String nativeLibraryFileName({String? operatingSystem}) {
  final system = operatingSystem ?? Platform.operatingSystem;
  return switch (system) {
    'windows' => 'atomusicator_bootstrap_ffi.dll',
    'macos' => 'libatomusicator_bootstrap_ffi.dylib',
    _ => 'libatomusicator_bootstrap_ffi.so',
  };
}

/// Android loads the bridge from the APK by soname. Desktop still searches paths.
bool nativeLibraryOpensByName({String? operatingSystem}) =>
    (operatingSystem ?? Platform.operatingSystem) == 'android';

String? firstExistingNativeLibrary({
  String? operatingSystem,
  Map<String, String>? environment,
  String? executableDirectory,
  String? workingDirectory,
  bool Function(String path)? exists,
}) {
  final name = nativeLibraryFileName(operatingSystem: operatingSystem);
  if (nativeLibraryOpensByName(operatingSystem: operatingSystem)) {
    return name;
  }
  final sep = Platform.pathSeparator;
  final env = environment ?? Platform.environment;
  final exeDir =
      executableDirectory ?? File(Platform.resolvedExecutable).parent.path;
  final cwd = workingDirectory ?? Directory.current.path;
  final present = exists ?? (path) => File(path).existsSync();
  final candidates = <String>[
    if ((env['ATOMUSICATOR_NATIVE_LIBRARY'] ?? '').isNotEmpty)
      env['ATOMUSICATOR_NATIVE_LIBRARY']!,
    '$exeDir$sep$name',
    '$cwd$sep$name',
    '$cwd${sep}rust_core${sep}target${sep}release$sep$name',
    '$cwd${sep}rust_core${sep}target${sep}debug$sep$name',
  ];
  for (final path in candidates) {
    if (present(path)) return path;
  }
  return null;
}

String requireNativeLibrary({
  String? operatingSystem,
  Map<String, String>? environment,
  String? executableDirectory,
  String? workingDirectory,
  bool Function(String path)? exists,
}) {
  final path = firstExistingNativeLibrary(
    operatingSystem: operatingSystem,
    environment: environment,
    executableDirectory: executableDirectory,
    workingDirectory: workingDirectory,
    exists: exists,
  );
  if (path != null) return path;
  throw StateError(
    'No se encontró ${nativeLibraryFileName(operatingSystem: operatingSystem)}. '
    'Coloca la DLL junto al ejecutable o define ATOMUSICATOR_NATIVE_LIBRARY. '
    'Un `flutter run` sin `packaging/build_windows.ps1 -PrepareDebug` no copia el puente Rust.',
  );
}
