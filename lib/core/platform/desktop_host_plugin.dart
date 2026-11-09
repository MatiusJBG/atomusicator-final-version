import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/widgets.dart';
import 'package:window_manager/window_manager.dart';

import '../theme/app_theme.dart';

Future<void> configureDesktopWindow() async {
  await windowManager.ensureInitialized();
  await windowManager.waitUntilReadyToShow(
    const WindowOptions(
      size: Size(1440, 900),
      minimumSize: Size(920, 640),
      center: true,
      title: 'Atomusicator',
      titleBarStyle: TitleBarStyle.hidden,
      windowButtonVisibility: false,
      backgroundColor: AppColors.canvas,
    ),
    () async {
      await windowManager.show();
      await windowManager.focus();
    },
  );
}

Widget dragToMove(Widget child) => DragToMoveArea(child: child);

Future<void> minimizeWindow() => windowManager.minimize();

Future<void> toggleMaximizeWindow() async {
  if (await windowManager.isMaximized()) {
    await windowManager.unmaximize();
  } else {
    await windowManager.maximize();
  }
}

Future<void> closeWindow() => windowManager.close();

Widget fileDrop({
  required Widget child,
  required VoidCallback onEntered,
  required VoidCallback onExited,
  required void Function(List<String> paths) onDone,
}) {
  return DropTarget(
    onDragEntered: (_) => onEntered(),
    onDragExited: (_) => onExited(),
    onDragDone:
        (details) => onDone([
          for (final file in details.files)
            if (file.path.isNotEmpty) file.path,
        ]),
    child: child,
  );
}
