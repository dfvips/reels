import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class UrlInput extends StatefulWidget {
  final Function(String) onParse;

  const UrlInput({super.key, required this.onParse});

  @override
  State<UrlInput> createState() => _UrlInputState();
}

class _UrlInputState extends State<UrlInput> {
  final TextEditingController _controller = TextEditingController();
  bool _hasContent = false;

  @override
  void initState() {
    super.initState();
    _controller.addListener(() {
      setState(() {
        _hasContent = _controller.text.isNotEmpty;
      });
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _pasteFromClipboard() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data != null && data.text != null) {
      _controller.text = data.text!;
    }
  }

  void _clearText() {
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    // 浅色模式颜色定义
    const Color lightDisabledColor = Color(0xFFC0C4CC);
    const Color lightDisabledBgColor = Color(0xFFFFFFFF);
    const Color lightDisabledBorderColor = Color(0xFFEBEEF5);
    const Color lightEnabledColor = Color(0xFF409EFF);
    const Color lightEnabledBgColor = Color(0xFFECF5FF);
    const Color lightEnabledBorderColor = Color(0xFFB3D8FF);

    // 深色模式颜色定义
    const Color darkPasteBgColor = Color(0xFF141414); // 粘贴按钮背景色
    const Color darkPasteBorderColor = Color(0xFF4C4D4F); // 粘贴按钮边框色
    const Color darkPasteTextColor = Colors.white; // 粘贴按钮文本色
    const Color darkClearBgColor = Color(0xFF18232B); // 清空按钮背景色
    const Color darkClearBorderColor = Color(0xFF409EFF); // 清空按钮边框色
    const Color darkClearTextColor = Color(0xFF409EFF); // 清空按钮文本色
    const Color darkDisabledColor = Color(0xFF4C4D4F);
    const Color darkDisabledBgColor = Color(0xFF141414);
    const Color darkDisabledBorderColor = Color(0xFF4C4D4F);

    return Card(
      color: isDarkMode ? Color(0xFF141414) : Colors.white,
      elevation: 0,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _controller,
              maxLines: 5,
              minLines: 3,
              decoration: InputDecoration(
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(
                    color: Theme.of(context).colorScheme.outline,
                  ),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(
                    color: Theme.of(context).colorScheme.outlineVariant,
                  ),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(
                    color: Theme.of(context).colorScheme.primary,
                  ),
                ),
                hintText: '输入或粘贴抖音视频分享链接',
                hintStyle: TextStyle(color: Color(0xFFC0C4CC)),
                contentPadding: const EdgeInsets.all(12),
              ),
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                // 粘贴按钮
                Expanded(
                  child: MouseRegion(
                    cursor: SystemMouseCursors.click,
                    child: Container(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        // boxShadow: [
                        //   BoxShadow(
                        //     color: Colors.black.withOpacity(0.05),
                        //     blurRadius: 2,
                        //     offset: const Offset(0, 1),
                        //   ),
                        // ],
                      ),
                      child: TextButton(
                        onPressed: _pasteFromClipboard,
                        style: TextButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          backgroundColor:
                              isDarkMode ? darkPasteBgColor : Colors.white,
                          foregroundColor:
                              isDarkMode
                                  ? darkPasteTextColor
                                  : lightEnabledColor,
                          side: BorderSide(
                            color:
                                isDarkMode
                                    ? darkPasteBorderColor
                                    : lightEnabledColor,
                            width: 1,
                          ),
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(4),
                          ),
                          overlayColor: lightEnabledBorderColor.withOpacity(
                            0.2,
                          ),
                        ),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(
                              Icons.paste,
                              size: 16,
                              color:
                                  isDarkMode
                                      ? darkPasteTextColor
                                      : lightEnabledColor,
                            ),
                            const SizedBox(width: 2),
                            Text('粘贴'),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                // 清空按钮
                Expanded(
                  child: MouseRegion(
                    cursor:
                        _hasContent
                            ? SystemMouseCursors.click
                            : SystemMouseCursors.basic,
                    child: Container(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        // boxShadow:
                        //     _hasContent
                        //         ? [
                        //           BoxShadow(
                        //             color: Colors.black.withOpacity(0.05),
                        //             blurRadius: 2,
                        //             offset: const Offset(0, 1),
                        //           ),
                        //         ]
                        //         : [],
                      ),
                      child: TextButton(
                        onPressed: _hasContent ? _clearText : null,
                        style: TextButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          backgroundColor:
                              _hasContent
                                  ? (isDarkMode
                                      ? darkClearBgColor
                                      : lightEnabledBgColor)
                                  : (isDarkMode
                                      ? darkDisabledBgColor
                                      : lightDisabledBgColor),
                          foregroundColor:
                              _hasContent
                                  ? (isDarkMode
                                      ? darkClearTextColor
                                      : lightEnabledColor)
                                  : (isDarkMode
                                      ? darkDisabledColor
                                      : lightDisabledColor),
                          side: BorderSide(
                            color:
                                _hasContent
                                    ? (isDarkMode
                                        ? darkClearBorderColor
                                        : lightEnabledBorderColor)
                                    : (isDarkMode
                                        ? darkDisabledBorderColor
                                        : lightDisabledBorderColor),
                            width: 1,
                          ),
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(4),
                          ),
                          disabledBackgroundColor:
                              isDarkMode
                                  ? darkDisabledBgColor
                                  : lightDisabledBgColor,
                          disabledForegroundColor:
                              isDarkMode
                                  ? darkDisabledColor
                                  : lightDisabledColor,
                        ),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(
                              Icons.restart_alt,
                              size: 20,
                              color:
                                  _hasContent
                                      ? (isDarkMode
                                          ? darkClearTextColor
                                          : lightEnabledColor)
                                      : (isDarkMode
                                          ? darkDisabledColor
                                          : lightDisabledColor),
                            ),
                            const SizedBox(width: 2),
                            Text(
                              '清空',
                              style: TextStyle(
                                color:
                                    _hasContent
                                        ? (isDarkMode
                                            ? darkClearTextColor
                                            : lightEnabledColor)
                                        : (isDarkMode
                                            ? darkDisabledColor
                                            : lightDisabledColor),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                // 解析按钮
                Expanded(
                  child: MouseRegion(
                    cursor:
                        _hasContent
                            ? SystemMouseCursors.click
                            : SystemMouseCursors.basic,
                    child: Container(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(4),
                        // boxShadow:
                        //     _hasContent
                        //         ? [
                        //           BoxShadow(
                        //             color: Colors.black.withOpacity(0.05),
                        //             blurRadius: 2,
                        //             offset: const Offset(0, 1),
                        //           ),
                        //         ]
                        //         : [],
                      ),
                      child: ElevatedButton(
                        onPressed:
                            _hasContent
                                ? () => widget.onParse(_controller.text)
                                : null,
                        style: ElevatedButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          backgroundColor:
                              _hasContent
                                  ? primaryColor
                                  : (isDarkMode
                                      ? darkDisabledBgColor
                                      : lightDisabledBgColor),
                          foregroundColor:
                              _hasContent
                                  ? (isDarkMode ? Colors.white : onPrimaryColor)
                                  : (isDarkMode
                                      ? darkDisabledColor
                                      : lightDisabledColor),
                          side: BorderSide(
                            color:
                                _hasContent
                                    ? primaryColor
                                    : (isDarkMode
                                        ? darkDisabledBorderColor
                                        : lightDisabledBorderColor),
                            width: _hasContent ? 0 : 1,
                          ),
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(4),
                          ),
                          elevation: 0,
                          disabledBackgroundColor:
                              isDarkMode
                                  ? darkDisabledBgColor
                                  : lightDisabledBgColor,
                          disabledForegroundColor:
                              isDarkMode
                                  ? darkDisabledColor
                                  : lightDisabledColor,
                        ),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Transform.translate(
                              offset: const Offset(0, -2),
                              child: Transform.rotate(
                                angle: -45 * (3.14159 / 180),
                                child: Icon(
                                  Icons.send,
                                  size: 14,
                                  color:
                                      _hasContent
                                          ? (isDarkMode
                                              ? Colors.white
                                              : onPrimaryColor)
                                          : (isDarkMode
                                              ? darkDisabledColor
                                              : lightDisabledColor),
                                ),
                              ),
                            ),
                            const SizedBox(width: 2),
                            Text(
                              '解析',
                              style: TextStyle(
                                color:
                                    _hasContent
                                        ? (isDarkMode
                                            ? Colors.white
                                            : onPrimaryColor)
                                        : (isDarkMode
                                            ? darkDisabledColor
                                            : lightDisabledColor),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
