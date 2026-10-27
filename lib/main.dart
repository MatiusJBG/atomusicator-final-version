import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';

import 'core/theme/app_theme.dart';
import 'src/features/workbench/audio_workbench.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    await windowManager.ensureInitialized();
    await windowManager.waitUntilReadyToShow(
      const WindowOptions(
        size: Size(1440, 920),
        minimumSize: Size(920, 640),
        center: true,
        title: 'Atomusicator',
        backgroundColor: AppColors.canvas,
      ),
      () async {
        await windowManager.show();
        await windowManager.focus();
      },
    );
  }
  runApp(const ProviderScope(child: AtomusicatorApp()));
}

class AtomusicatorApp extends StatelessWidget {
  const AtomusicatorApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Atomusicator',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.dark,
      home: const AudioWorkbench(),
    );
  }
}
