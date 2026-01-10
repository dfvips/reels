import 'package:bot_toast/bot_toast.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class ResultDisplay extends StatefulWidget {
  final List<String> results; // 现在这个是视频链接列表
  final List<String> titles; // 新增：视频标题列表
  final VoidCallback onCopy;
  final VoidCallback onDownload;

  const ResultDisplay({
    super.key,
    required this.results,
    required this.titles,
    required this.onCopy,
    required this.onDownload,
  });

  @override
  State<ResultDisplay> createState() => _ResultDisplayState();
}

class _ResultDisplayState extends State<ResultDisplay> {
  // 创建独立的 ScrollController
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    // 浅色模式颜色定义
    const Color lightEnabledColor = Color(0xFF409EFF);
    // 深色模式颜色定义
    const Color darkCopyBgColor = Color(0xFF141414);
    const Color darkCopyBorderColor = Color(0xFF4C4D4F);
    const Color darkCopyTextColor = Colors.white;

    return Card(
      color: isDarkMode ? const Color(0xFF141414) : Colors.white,
      elevation: 0,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  '解析结果 (${widget.results.length}个):',
                  style: TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.bold,
                    color: primaryColor,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            // 使用 LayoutBuilder 获取父容器约束并响应窗口变化
            LayoutBuilder(
              builder: (context, constraints) {
                // 动态计算最大允许高度：屏幕高度的 1/3，但限制在合理范围
                final screenHeight = MediaQuery.of(context).size.height;
                double maxHeight = screenHeight / 3;
                const double minHeight = 80.0;
                const double maxHeightLimit = 400.0;
                maxHeight = maxHeight.clamp(minHeight, maxHeightLimit);

                // 判断是否需要滚动（这里简化为数量 > 5，也可按像素估算）
                final needsScroll = widget.results.length > 5;

                return _buildResultContainer(
                  context,
                  maxHeight: maxHeight,
                  needsScroll: needsScroll,
                );
              },
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                // 复制所有链接按钮
                Expanded(
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
                    child: TextButton.icon(
                      onPressed: () {
                        // 复制所有结果，格式为：标题\n链接
                        final allResults = [];
                        for (int i = 0; i < widget.results.length; i++) {
                          final title = widget.titles[i];
                          final link = widget.results[i];
                          allResults.add('$title\n$link');
                        }
                        _copyAllLinks(allResults.join('\n'));
                      },
                      style: TextButton.styleFrom(
                        padding: const EdgeInsets.symmetric(vertical: 12),
                        backgroundColor:
                            isDarkMode ? darkCopyBgColor : Colors.white,
                        foregroundColor:
                            isDarkMode ? darkCopyTextColor : lightEnabledColor,
                        side: BorderSide(
                          color:
                              isDarkMode
                                  ? darkCopyBorderColor
                                  : lightEnabledColor,
                          width: 1,
                        ),
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(4),
                        ),
                        overlayColor: lightEnabledColor.withOpacity(0.2),
                      ),
                      icon: Icon(
                        Icons.copy_all,
                        size: 16,
                        color:
                            isDarkMode ? darkCopyTextColor : lightEnabledColor,
                      ),
                      label: Text('复制链接 (${widget.results.length})'),
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                // 下载所有按钮
                Expanded(
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
                    child: ElevatedButton.icon(
                      onPressed: widget.onDownload,
                      style: ElevatedButton.styleFrom(
                        padding: const EdgeInsets.symmetric(vertical: 12),
                        backgroundColor: primaryColor,
                        foregroundColor:
                            isDarkMode ? Colors.white : onPrimaryColor,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(4),
                        ),
                        elevation: 0,
                      ),
                      icon: Icon(
                        Icons.download,
                        size: 16,
                        color: isDarkMode ? Colors.white : onPrimaryColor,
                      ),
                      label: Text('下载视频 (${widget.results.length})'),
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

  // 构建结果容器
  Widget _buildResultContainer(
    BuildContext context, {
    required double maxHeight,
    required bool needsScroll,
  }) {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;

    return AnimatedContainer(
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeInOut,
      // 不设置固定 height，让内容决定高度，但限制最大高度
      constraints: BoxConstraints(maxHeight: maxHeight),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: isDarkMode ? const Color(0xFF1A222C) : const Color(0xFFEEF5FF),
        borderRadius: BorderRadius.circular(8),
      ),
      child:
          needsScroll
              ? RawScrollbar(
                controller: _scrollController,
                thumbVisibility: true,
                thumbColor: primaryColor.withOpacity(0.7),
                radius: const Radius.circular(10),
                thickness: 8,
                minThumbLength: 50,
                child: _buildResultList(isDarkMode, primaryColor),
              )
              : _buildResultList(isDarkMode, primaryColor),
    );
  }

  // 构建结果列表
  Widget _buildResultList(bool isDarkMode, Color primaryColor) {
    return ListView.builder(
      controller: _scrollController,
      shrinkWrap: true, // 关键：让 ListView 高度由内容决定
      physics: const AlwaysScrollableScrollPhysics(), // 允许滚动（即使内容少，也支持拖拽）
      itemCount: widget.results.length,
      itemBuilder: (context, index) {
        final title = widget.titles[index];
        final link = widget.results[index];
        final displayText = '$title\n$link';

        return Padding(
          padding: EdgeInsets.only(
            bottom: index < widget.results.length - 1 ? 12 : 0,
          ),
          child: GestureDetector(
            onTap: () async {
              await _copySingleLink(context, title, link);
            },
            child: Container(
              padding: const EdgeInsets.all(8),
              decoration: BoxDecoration(
                color:
                    isDarkMode ? Colors.black.withOpacity(0.3) : Colors.white,
                borderRadius: BorderRadius.circular(6),
              ),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 24,
                    height: 24,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: primaryColor.withOpacity(0.1),
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Text(
                      '${index + 1}',
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.bold,
                        color: primaryColor,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: SelectableText(
                      displayText,
                      style: const TextStyle(
                        fontFamily: 'monospace',
                        fontSize: 12,
                      ),
                    ),
                  ),
                  MouseRegion(
                    cursor: SystemMouseCursors.click,
                    child: Icon(
                      Icons.content_copy,
                      size: 16,
                      color: primaryColor,
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  Future<void> _copySingleLink(
    BuildContext context,
    String title,
    String link,
  ) async {
    final displayText = '$title\n$link';
    await Clipboard.setData(ClipboardData(text: displayText));

    BotToast.showText(
      text: '已复制到剪贴板',
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
  }

  Future<void> _copyAllLinks(String allLinks) async {
    await Clipboard.setData(ClipboardData(text: allLinks));

    BotToast.showText(
      text: '已复制所有链接到剪贴板',
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
  }
}
