import 'dart:io';

import 'package:flutter/widgets.dart';

bool get isDesktopHost =>
    Platform.isWindows || Platform.isLinux || Platform.isMacOS;

bool isPhoneLayout(BuildContext context) {
  final media = MediaQuery.sizeOf(context);
  final orientation = MediaQuery.orientationOf(context);
  return orientation == Orientation.portrait && media.shortestSide < 600 ||
      media.width < 600;
}

bool isCompactLandscape(BuildContext context) {
  final media = MediaQuery.sizeOf(context);
  return media.width >= 600 && media.height < 540;
}
