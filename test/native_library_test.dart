import 'package:atomusicator/core/di/native_library.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('prefers ATOMUSICATOR_NATIVE_LIBRARY when the file exists', () {
    expect(
      firstExistingNativeLibrary(
        environment: {
          'ATOMUSICATOR_NATIVE_LIBRARY':
              r'C:\prepared\atomusicator_bootstrap_ffi.dll',
        },
        executableDirectory: r'C:\flutter-debug',
        workingDirectory: r'C:\repo',
        exists: (path) => path.endsWith('atomusicator_bootstrap_ffi.dll'),
      ),
      r'C:\prepared\atomusicator_bootstrap_ffi.dll',
    );
  });

  test('falls back to the executable directory before cargo targets', () {
    expect(
      firstExistingNativeLibrary(
        environment: const {},
        executableDirectory: r'C:\app',
        workingDirectory: r'C:\repo',
        exists:
            (path) =>
                path == r'C:\app\atomusicator_bootstrap_ffi.dll' ||
                path.contains(r'rust_core\target\release'),
      ),
      r'C:\app\atomusicator_bootstrap_ffi.dll',
    );
  });

  test('android opens the bridge by soname from the APK', () {
    expect(
      nativeLibraryFileName(operatingSystem: 'android'),
      'libatomusicator_bootstrap_ffi.so',
    );
    expect(
      firstExistingNativeLibrary(
        operatingSystem: 'android',
        environment: const {},
        executableDirectory: r'/data/app',
        workingDirectory: r'/data/user/0',
        exists: (_) => false,
      ),
      'libatomusicator_bootstrap_ffi.so',
    );
    expect(
      requireNativeLibrary(
        operatingSystem: 'android',
        environment: const {},
        exists: (_) => false,
      ),
      'libatomusicator_bootstrap_ffi.so',
    );
  });

  test('explains a missing bridge instead of opening a stale path', () {
    expect(
      () => requireNativeLibrary(
        environment: const {},
        executableDirectory: r'C:\missing',
        workingDirectory: r'C:\repo',
        exists: (_) => false,
      ),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          contains('ATOMUSICATOR_NATIVE_LIBRARY'),
        ),
      ),
    );
  });
}
