import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'core/di/desktop_studio_state.dart';
import 'core/platform/desktop_window.dart';
import 'core/theme/app_theme.dart';
import 'src/features/workbench/audio_workbench.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (Platform.isAndroid || Platform.isIOS) {
    await SystemChrome.setPreferredOrientations([
      DeviceOrientation.landscapeLeft,
      DeviceOrientation.landscapeRight,
    ]);
    await SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
  }
  if (Platform.isAndroid) {
    await prepareApplicationPreferences();
  }
  await configureDesktopWindow();
  runApp(const ProviderScope(child: AtomusicatorApp()));
}

class AtomusicatorApp extends ConsumerWidget {
  const AtomusicatorApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(desktopPreferencesLoadProvider);
    return MaterialApp(
      title: 'Atomusicator',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.dark,
      home: const AudioWorkbench(),
    );
  }
}
