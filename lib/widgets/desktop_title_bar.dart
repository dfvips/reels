import 'dart:io';

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

/// 桌面端自绘标题栏。
///
/// 顶部为系统按钮预留条：macOS 上 `TitleBarStyle.hidden` 保留系统红绿灯，
/// Windows 上放置自绘的最小化/最大化/关闭按钮；预留条可拖拽窗口，
/// 双击在最大化与还原之间切换（macOS 下是 zoom，不进入全屏）。
/// 下方为应用标题行，同样支持拖拽。
class DesktopTitleBar extends StatefulWidget {
  const DesktopTitleBar({
    super.key,
    required this.title,
    this.actions = const <Widget>[],
    this.leading,
  });

  /// 系统按钮预留条高度（Windows 自绘按钮的默认最小高度为 32）
  static const double systemBarHeight = 32;

  /// 应用标题行高度
  static const double titleRowHeight = 44;

  /// 标题栏总高度
  static const double totalHeight = systemBarHeight + titleRowHeight;

  final Widget title;
  final List<Widget> actions;
  final Widget? leading;

  @override
  State<DesktopTitleBar> createState() => _DesktopTitleBarState();
}

class _DesktopTitleBarState extends State<DesktopTitleBar> with WindowListener {
  bool _isMaximized = false;

  bool get _isWindows => Platform.isWindows;

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
    _refreshMaximized();
  }

  @override
  void dispose() {
    windowManager.removeListener(this);
    super.dispose();
  }

  Future<void> _refreshMaximized() async {
    try {
      final maximized = await windowManager.isMaximized();
      if (mounted && maximized != _isMaximized) {
        setState(() => _isMaximized = maximized);
      }
    } catch (_) {
      // 测试环境或插件未就绪时忽略
    }
  }

  @override
  void onWindowMaximize() {
    setState(() => _isMaximized = true);
  }

  @override
  void onWindowUnmaximize() {
    setState(() => _isMaximized = false);
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _buildSystemBar(),
        _buildTitleRow(),
      ],
    );
  }

  /// 系统按钮预留条：macOS 红绿灯由系统悬浮在此区域，
  /// Windows 放自绘按钮；整条透明、可拖拽、可双击最大化。
  Widget _buildSystemBar() {
    return Container(
      height: DesktopTitleBar.systemBarHeight,
      color: Colors.transparent,
      child: Row(
        children: [
          Expanded(child: DragToMoveArea(child: const SizedBox.expand())),
          if (_isWindows) _buildWindowsCaptionButtons(),
          const SizedBox(width: 8),
        ],
      ),
    );
  }

  Widget _buildTitleRow() {
    return Container(
      height: DesktopTitleBar.titleRowHeight,
      color: Colors.transparent,
      padding: const EdgeInsets.only(left: 12),
      child: Row(
        children: [
          if (widget.leading != null) widget.leading!,
          Expanded(
            child: DragToMoveArea(
              child: Row(
                children: [
                  Expanded(child: widget.title),
                  ...widget.actions,
                ],
              ),
            ),
          ),
          const SizedBox(width: 8),
        ],
      ),
    );
  }

  Widget _buildWindowsCaptionButtons() {
    final brightness = Theme.of(context).brightness;
    return Row(
      children: [
        _CaptionButton(
          type: _CaptionButtonType.minimize,
          brightness: brightness,
          onPressed: () => windowManager.minimize(),
        ),
        _CaptionButton(
          type: _isMaximized
              ? _CaptionButtonType.restore
              : _CaptionButtonType.maximize,
          brightness: brightness,
          onPressed: () => _isMaximized
              ? windowManager.unmaximize()
              : windowManager.maximize(),
        ),
        _CaptionButton(
          type: _CaptionButtonType.close,
          brightness: brightness,
          onPressed: () => windowManager.close(),
        ),
      ],
    );
  }
}

enum _CaptionButtonType { minimize, maximize, restore, close }

/// Windows 自绘标题栏按钮：普通按钮深/浅色模式固定底色，
/// 关闭按钮 hover 始终红底白图标。
class _CaptionButton extends StatefulWidget {
  const _CaptionButton({
    required this.type,
    required this.brightness,
    required this.onPressed,
  });

  final _CaptionButtonType type;
  final Brightness brightness;
  final VoidCallback onPressed;

  @override
  State<_CaptionButton> createState() => _CaptionButtonState();
}

class _CaptionButtonState extends State<_CaptionButton> {
  bool _hovering = false;
  bool _pressed = false;

  @override
  Widget build(BuildContext context) {
    final isDark = widget.brightness == Brightness.dark;
    final isClose = widget.type == _CaptionButtonType.close;
    Color background;
    final Color iconColor;
    if (isClose) {
      background =
          _hovering || _pressed ? const Color(0xFFC42C1B) : Colors.transparent;
      iconColor =
          _hovering || _pressed || isDark ? Colors.white : const Color(0xFF1A1A1A);
    } else {
      background = isDark ? const Color(0xFF2D2D2D) : const Color(0xFFE5E5E5);
      if (_hovering) {
        background = isDark ? const Color(0xFF383838) : const Color(0xFFDADADA);
      }
      if (_pressed) {
        background = isDark ? const Color(0xFF333333) : const Color(0xFFDFDFDF);
      }
      iconColor = isDark ? Colors.white : const Color(0xFF1A1A1A);
    }

    return MouseRegion(
      onExit: (_) => setState(() {
        _hovering = false;
        _pressed = false;
      }),
      onHover: (_) => setState(() => _hovering = true),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (_) => setState(() => _pressed = true),
        onTapCancel: () => setState(() => _pressed = false),
        onTapUp: (_) => setState(() => _pressed = false),
        onTap: widget.onPressed,
        child: Container(
          constraints: const BoxConstraints(minWidth: 46, minHeight: 32),
          color: background,
          child: Center(
            child: CustomPaint(
              size: const Size(16, 16),
              painter: _CaptionIconPainter(widget.type, iconColor),
            ),
          ),
        ),
      ),
    );
  }
}

class _CaptionIconPainter extends CustomPainter {
  const _CaptionIconPainter(this.type, this.color);

  final _CaptionButtonType type;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    switch (type) {
      case _CaptionButtonType.minimize:
        canvas.drawLine(const Offset(4, 8), const Offset(12, 8), paint);
      case _CaptionButtonType.maximize:
        canvas.drawRect(const Rect.fromLTWH(4, 4, 8, 8), paint);
      case _CaptionButtonType.restore:
        canvas.drawRect(const Rect.fromLTWH(6, 3, 7, 7), paint);
        canvas.drawRect(const Rect.fromLTWH(3, 6, 7, 7), paint);
      case _CaptionButtonType.close:
        canvas.drawLine(const Offset(4, 4), const Offset(12, 12), paint);
        canvas.drawLine(const Offset(12, 4), const Offset(4, 12), paint);
    }
  }

  @override
  bool shouldRepaint(_CaptionIconPainter oldDelegate) =>
      oldDelegate.type != type || oldDelegate.color != color;
}
