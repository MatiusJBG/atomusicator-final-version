import 'package:flutter/widgets.dart';

import 'desktop_host_plugin.dart' deferred as host;
import 'host_platform.dart';

Widget Function(Widget child)? _drag;
Future<void> Function()? _minimize;
Future<void> Function()? _toggleMaximize;
Future<void> Function()? _close;
Widget Function({
  required Widget child,
  required VoidCallback onEntered,
  required VoidCallback onExited,
  required void Function(List<String> paths) onDone,
})?
_drop;

/// Loads window and drag libraries only on desktop. Android never executes them.
Future<void> configureDesktopWindow() async {
  if (!isDesktopHost) return;
  await host.loadLibrary();
  _drag = host.dragToMove;
  _minimize = host.minimizeWindow;
  _toggleMaximize = host.toggleMaximizeWindow;
  _close = host.closeWindow;
  _drop = host.fileDrop;
  await host.configureDesktopWindow();
}

/// Initializes file drop handler for desktop-specific unit and widget tests.
@visibleForTesting
Future<void> initDesktopFileDropForTesting() async {
  if (!isDesktopHost) return;
  await host.loadLibrary();
  _drop = host.fileDrop;
}

Widget desktopDragArea({required Widget child}) => _drag?.call(child) ?? child;

Future<void> minimizeApplicationWindow() async {
  await _minimize?.call();
}

Future<void> toggleMaximizeApplicationWindow() async {
  await _toggleMaximize?.call();
}

Future<void> closeApplicationWindow() async {
  await _close?.call();
}

class DesktopFileDrop extends StatelessWidget {
  const DesktopFileDrop({
    super.key,
    required this.child,
    required this.onEntered,
    required this.onExited,
    required this.onDone,
  });

  final Widget child;
  final VoidCallback onEntered;
  final VoidCallback onExited;
  final void Function(List<String> paths) onDone;

  @override
  Widget build(BuildContext context) {
    final drop = _drop;
    if (!isDesktopHost || drop == null) return child;
    return drop(
      child: child,
      onEntered: onEntered,
      onExited: onExited,
      onDone: onDone,
    );
  }
}
