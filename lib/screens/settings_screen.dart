import 'dart:io';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:file_picker/file_picker.dart';
import 'package:bot_toast/bot_toast.dart';
import 'package:douyin_parser/widgets/desktop_title_bar.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  @override
  void initState() {
    super.initState();
    // 初始化时刷新缓存大小
    WidgetsBinding.instance.addPostFrameCallback((_) {
      Provider.of<AppSettings>(context, listen: false).refreshCacheSize();
    });
  }

  @override
  Widget build(BuildContext context) {
    final settings = Provider.of<AppSettings>(context);
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;

    // 设置全局背景色
    final backgroundColor = isDarkMode ? Color(0xFF141414) : Colors.white;

    return Scaffold(
      appBar: _buildAppBar(isDarkMode),
      backgroundColor: backgroundColor, // 设置页面背景色
      body: Container(
        color: backgroundColor, // 设置列表容器背景色
        child: ListView(
          children: [
            const SizedBox(height: 8),

            // 保存位置设置（仅桌面端显示）
            if (Platform.isMacOS || Platform.isWindows || Platform.isLinux)
              Column(
                children: [
                  // 保存位置项 - 修改为与主题设置相同样式
                  Material(
                    color: backgroundColor,
                    child: InkWell(
                      onTap: () {
                        _showSaveLocationSettings(context);
                      },
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 16,
                          vertical: 12,
                        ),
                        child: Row(
                          children: [
                            Container(
                              margin: const EdgeInsets.only(right: 16),
                              child: Icon(
                                Icons.folder,
                                color: Theme.of(context).colorScheme.primary,
                                size: 24,
                              ),
                            ),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                    '存储设置',
                                    style: TextStyle(
                                      fontSize: 16,
                                      fontWeight: FontWeight.w500,
                                      color:
                                          isDarkMode
                                              ? Colors.white
                                              : Colors.black,
                                    ),
                                  ),
                                  const SizedBox(height: 4),
                                  Text(
                                    '设置视频保存的默认位置',
                                    style: TextStyle(
                                      fontSize: 14,
                                      color:
                                          isDarkMode
                                              ? Colors.white.withOpacity(0.7)
                                              : Color(0xFF606266),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  Divider(
                    height: 1,
                    color: isDarkMode ? Color(0xFF333333) : Color(0xFFEBEEF5),
                  ),
                ],
              ),

            // 主题设置 - 修改为可点击整个项
            Material(
              color: backgroundColor, // 使用全局背景色
              child: InkWell(
                onTap: () {
                  _showThemeDialog(context, settings);
                },
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 12,
                  ),
                  child: Row(
                    children: [
                      Container(
                        margin: const EdgeInsets.only(right: 16),
                        child: Icon(
                          Icons.color_lens,
                          color: Theme.of(context).colorScheme.primary,
                          size: 24,
                        ),
                      ),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '主题设置',
                              style: TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w500,
                                color: isDarkMode ? Colors.white : Colors.black,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              '当前: ${settings.themeName}',
                              style: TextStyle(
                                fontSize: 14,
                                color:
                                    isDarkMode
                                        ? Colors.white.withOpacity(0.7)
                                        : Color(0xFF606266),
                              ),
                            ),
                          ],
                        ),
                      ),
                      // 移除右侧箭头图标，因为没有下级菜单
                    ],
                  ),
                ),
              ),
            ),
            Divider(
              height: 1,
              color: isDarkMode ? Color(0xFF333333) : Color(0xFFEBEEF5),
            ),

            // 关于 - 修改为与主题设置相同样式
            Material(
              color: backgroundColor,
              child: InkWell(
                onTap: () {
                  _showAboutDialog(context);
                },
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 12,
                  ),
                  child: Row(
                    children: [
                      Container(
                        margin: const EdgeInsets.only(right: 16),
                        child: Icon(
                          Icons.info_outline,
                          color: Theme.of(context).colorScheme.primary,
                          size: 24,
                        ),
                      ),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '关于',
                              style: TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w500,
                                color: isDarkMode ? Colors.white : Colors.black,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              '应用版本: 20260110',
                              style: TextStyle(
                                fontSize: 14,
                                color:
                                    isDarkMode
                                        ? Colors.white.withOpacity(0.7)
                                        : Color(0xFF606266),
                              ),
                            ),
                          ],
                        ),
                      ),
                      // 移除右侧箭头图标，因为没有下级菜单
                    ],
                  ),
                ),
              ),
            ),
            Divider(
              height: 1,
              color: isDarkMode ? Color(0xFF333333) : Color(0xFFEBEEF5),
            ),

            // 清除缓存 - 显示缓存大小和位置
            Material(
              color: backgroundColor, // 使用全局背景色
              child: InkWell(
                onTap: () {
                  _showClearCacheDialog(context);
                },
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 12,
                  ),
                  child: Row(
                    children: [
                      Container(
                        margin: const EdgeInsets.only(right: 16),
                        child: Icon(
                          Icons.delete_outline,
                          color: Theme.of(context).colorScheme.primary,
                          size: 24,
                        ),
                      ),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '清除缓存',
                              style: TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w500,
                                color: isDarkMode ? Colors.white : Colors.black,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Consumer<AppSettings>(
                              builder: (context, settings, child) {
                                return Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      '当前缓存: ${settings.cacheSize}',
                                      style: TextStyle(
                                        fontSize: 14,
                                        color:
                                            isDarkMode
                                                ? Colors.white.withOpacity(0.7)
                                                : Color(0xFF606266),
                                      ),
                                    ),
                                    if (settings.cachePaths.isNotEmpty)
                                      Text(
                                        '${settings.cachePaths.length}个位置',
                                        style: TextStyle(
                                          fontSize: 12,
                                          color:
                                              isDarkMode
                                                  ? Colors.white.withOpacity(
                                                    0.5,
                                                  )
                                                  : Color(0xFF909399),
                                        ),
                                      ),
                                  ],
                                );
                              },
                            ),
                          ],
                        ),
                      ),
                      // 移除右侧箭头图标，因为没有下级菜单
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  PreferredSizeWidget _buildAppBar(bool isDarkMode) {
    if (Platform.isWindows || Platform.isMacOS) {
      return PreferredSize(
        preferredSize: const Size.fromHeight(DesktopTitleBar.totalHeight),
        child: DesktopTitleBar(
          leading: IconButton(
            icon: const Icon(Icons.arrow_back),
            onPressed: () => Navigator.of(context).pop(),
          ),
          title: const Center(
            child: Text(
              '设置',
              style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
            ),
          ),
        ),
      );
    }

    return AppBar(
      title: const Text('设置'),
      centerTitle: true,
      backgroundColor: isDarkMode ? Color(0xFF141414) : Colors.white,
    );
  }

  // 显示主题对话框
  void _showThemeDialog(BuildContext context, AppSettings settings) {
    showDialog(
      context: context,
      builder: (context) {
        int? selectedIndex;

        // 根据当前主题设置默认选中项
        switch (settings.themeMode) {
          case ThemeMode.system:
            selectedIndex = 0;
            break;
          case ThemeMode.light:
            selectedIndex = 1;
            break;
          case ThemeMode.dark:
            selectedIndex = 2;
            break;
        }

        return StatefulBuilder(
          builder: (context, setState) {
            return AlertDialog(
              title: const Text('选择主题'),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  // 跟随系统选项
                  Material(
                    color: Colors.transparent,
                    child: InkWell(
                      onTap: () {
                        setState(() {
                          selectedIndex = 0;
                        });
                        settings.setThemeMode(ThemeMode.system, '跟随系统');
                        Navigator.pop(context);
                      },
                      child: Container(
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        child: Row(
                          children: [
                            Radio<int>(
                              value: 0,
                              groupValue: selectedIndex,
                              onChanged: (value) {
                                setState(() {
                                  selectedIndex = value;
                                });
                                settings.setThemeMode(ThemeMode.system, '跟随系统');
                                Navigator.pop(context);
                              },
                            ),
                            const SizedBox(width: 8),
                            const Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('跟随系统', style: TextStyle(fontSize: 16)),
                                  SizedBox(height: 2),
                                  Text(
                                    '根据系统设置自动切换',
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: Colors.grey,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const Divider(height: 1),

                  // 浅色模式选项
                  Material(
                    color: Colors.transparent,
                    child: InkWell(
                      onTap: () {
                        setState(() {
                          selectedIndex = 1;
                        });
                        settings.setThemeMode(ThemeMode.light, '浅色模式');
                        Navigator.pop(context);
                      },
                      child: Container(
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        child: Row(
                          children: [
                            Radio<int>(
                              value: 1,
                              groupValue: selectedIndex,
                              onChanged: (value) {
                                setState(() {
                                  selectedIndex = value;
                                });
                                settings.setThemeMode(ThemeMode.light, '浅色模式');
                                Navigator.pop(context);
                              },
                            ),
                            const SizedBox(width: 8),
                            const Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('浅色模式', style: TextStyle(fontSize: 16)),
                                  SizedBox(height: 2),
                                  Text(
                                    '使用明亮的主题',
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: Colors.grey,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const Divider(height: 1),

                  // 深色模式选项
                  Material(
                    color: Colors.transparent,
                    child: InkWell(
                      onTap: () {
                        setState(() {
                          selectedIndex = 2;
                        });
                        settings.setThemeMode(ThemeMode.dark, '深色模式');
                        Navigator.pop(context);
                      },
                      child: Container(
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        child: Row(
                          children: [
                            Radio<int>(
                              value: 2,
                              groupValue: selectedIndex,
                              onChanged: (value) {
                                setState(() {
                                  selectedIndex = value;
                                });
                                settings.setThemeMode(ThemeMode.dark, '深色模式');
                                Navigator.pop(context);
                              },
                            ),
                            const SizedBox(width: 8),
                            const Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('深色模式', style: TextStyle(fontSize: 16)),
                                  SizedBox(height: 2),
                                  Text(
                                    '使用深色的主题',
                                    style: TextStyle(
                                      fontSize: 12,
                                      color: Colors.grey,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('关闭'),
                ),
              ],
            );
          },
        );
      },
    );
  }

  // 显示清除缓存对话框
  void _showClearCacheDialog(BuildContext context) {
    final settings = Provider.of<AppSettings>(context, listen: false);
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('清除缓存'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('确定要清除所有缓存数据吗？'),
              const SizedBox(height: 12),
              Consumer<AppSettings>(
                builder: (context, settings, child) {
                  return Text(
                    '当前缓存大小: ${settings.cacheSize}',
                    style: TextStyle(
                      fontSize: 14,
                      color: Theme.of(context).colorScheme.secondary,
                    ),
                  );
                },
              ),
              const SizedBox(height: 8),
              const Text(
                '将清理以下位置的缓存:',
                style: TextStyle(fontSize: 12, color: Colors.grey),
              ),
              const SizedBox(height: 4),
              Consumer<AppSettings>(
                builder: (context, settings, child) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children:
                        settings.cachePaths.map((path) {
                          return Text(
                            '• $path',
                            style: const TextStyle(
                              fontSize: 11,
                              color: Colors.grey,
                            ),
                          );
                        }).toList(),
                  );
                },
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('取消'),
            ),
            Container(
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
              child: ElevatedButton(
                onPressed: () async {
                  Navigator.pop(context);

                  // 显示清理中的提示
                  BotToast.showLoading();

                  try {
                    final result = await settings.clearCache();

                    BotToast.closeAllLoading();

                    // 显示清理结果
                    _showCleanResultDialog(context, result);
                  } catch (e) {
                    BotToast.closeAllLoading();
                    BotToast.showText(
                      text: '清除缓存失败: $e',
                      duration: const Duration(seconds: 3),
                      contentColor: Colors.red,
                    );
                  }
                },
                style: ElevatedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    vertical: 12,
                    horizontal: 16,
                  ),
                  backgroundColor: primaryColor,
                  foregroundColor: isDarkMode ? Colors.white : onPrimaryColor,
                  side: BorderSide.none,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(16),
                  ),
                  elevation: 0,
                ),
                child: const Text('清理'),
              ),
            ),
          ],
        );
      },
    );
  }

  // 显示清理结果对话框
  void _showCleanResultDialog(
    BuildContext context,
    Map<String, dynamic> result,
  ) {
    final bool success = result['success'] as bool;
    final String message = result['message'] as String;
    final List<String> cleanedPaths =
        (result['cleanedPaths'] as List<dynamic>).cast<String>();

    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: Row(
            children: [
              Icon(
                success ? Icons.check_circle : Icons.error,
                color: success ? Colors.green : Colors.red,
                size: 24,
              ),
              const SizedBox(width: 8),
              Text(success ? '清理完成' : '清理失败'),
            ],
          ),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  message,
                  style: TextStyle(
                    color: success ? Colors.green : Colors.red,
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const SizedBox(height: 12),

                if (cleanedPaths.isNotEmpty)
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Text(
                        '清理详情:',
                        style: TextStyle(fontWeight: FontWeight.bold),
                      ),
                      const SizedBox(height: 8),
                      Container(
                        constraints: const BoxConstraints(maxHeight: 200),
                        child: SingleChildScrollView(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children:
                                cleanedPaths.map((detail) {
                                  final isError = detail.contains('❌');
                                  final isSuccess = detail.contains('🗑️');

                                  return Padding(
                                    padding: const EdgeInsets.only(bottom: 4),
                                    child: Text(
                                      detail,
                                      style: TextStyle(
                                        fontSize: 12,
                                        color:
                                            isError
                                                ? Colors.red
                                                : isSuccess
                                                ? Colors.green
                                                : Colors.grey,
                                      ),
                                    ),
                                  );
                                }).toList(),
                          ),
                        ),
                      ),
                    ],
                  )
                else
                  const Text(
                    '未发现需要清理的缓存文件',
                    style: TextStyle(color: Colors.grey),
                  ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('确定'),
            ),
          ],
        );
      },
    );
  }

  // 显示保存位置设置对话框 - 修复版本
  static Future<void> _showSaveLocationSettings(BuildContext context) async {
    try {
      // 获取当前设置
      final currentLocation = await DownloadService.getCurrentSaveLocation();

      // 读取用户偏好设置
      final skipLocationPrompt = await DownloadService.getSkipLocationPrompt();
      await DownloadService.getSkipFilenameCustomization();

      bool tempSkipLocationPrompt = skipLocationPrompt;

      // 获取主题色
      final isDarkMode = Theme.of(context).brightness == Brightness.dark;
      final primaryColor = Theme.of(context).colorScheme.primary;
      final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

      // 深色模式颜色定义
      const Color darkClearBgColor = Color(0xFF18232B); // 清空按钮背景色
      const Color darkClearBorderColor = Color(0xFF409EFF); // 清空按钮边框色
      const Color darkClearTextColor = Color(0xFF409EFF); // 清空按钮文本色
      const Color lightEnabledBgColor = Color(0xFFECF5FF);

      final result = await showDialog<Map<String, dynamic>>(
        context: context,
        builder: (context) {
          return StatefulBuilder(
            builder: (context, setState) {
              return AlertDialog(
                title: const Text('存储设置'),
                content: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // 当前保存位置
                      const Text('当前保存位置:'),
                      Column(
                        children: [
                          const SizedBox(height: 4),
                          // 使用 IntrinsicWidth 可以强制子元素适应 Column 的宽度
                          IntrinsicWidth(
                            child: Container(
                              width: 300, // 确保 Container 尽可能宽
                              padding: const EdgeInsets.all(8),
                              decoration: BoxDecoration(
                                color:
                                    isDarkMode
                                        ? const Color(0xFF1A222C)
                                        : const Color(0xFFEEF5FF),
                                borderRadius: BorderRadius.circular(4),
                              ),
                              child: Text(
                                currentLocation ?? '未设置',
                                style: TextStyle(
                                  fontSize: 12,
                                  color:
                                      isDarkMode
                                          ? Colors.white
                                          : const Color(0xFF409eff),
                                ),
                                softWrap: true,
                              ),
                            ),
                          ),
                          const SizedBox(height: 16),
                        ],
                      ),

                      // 按钮行：更改和重置
                      Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          // 更改按钮 - 使用解析按钮样式
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
                                child: ElevatedButton(
                                  onPressed: () async {
                                    Navigator.of(
                                      context,
                                    ).pop({'action': 'change_location'});
                                  },
                                  style: ElevatedButton.styleFrom(
                                    padding: const EdgeInsets.symmetric(
                                      vertical: 12,
                                    ),
                                    backgroundColor: primaryColor,
                                    foregroundColor:
                                        isDarkMode
                                            ? Colors.white
                                            : onPrimaryColor,
                                    side: BorderSide.none,
                                    shape: RoundedRectangleBorder(
                                      borderRadius: BorderRadius.circular(16),
                                    ),
                                    elevation: 0,
                                  ),
                                  child: const Text('更改'),
                                ),
                              ),
                            ),
                          ),

                          const SizedBox(width: 16),
                          // 重置按钮 - 使用清空按钮样式
                          Expanded(
                            child: MouseRegion(
                              cursor: SystemMouseCursors.click,
                              child: Container(
                                decoration: BoxDecoration(
                                  borderRadius: BorderRadius.circular(4),
                                ),
                                child: TextButton(
                                  onPressed: () async {
                                    Navigator.of(
                                      context,
                                    ).pop({'action': 'reset'});
                                  },
                                  style: TextButton.styleFrom(
                                    padding: const EdgeInsets.symmetric(
                                      vertical: 12,
                                    ),
                                    backgroundColor:
                                        isDarkMode
                                            ? darkClearBgColor
                                            : lightEnabledBgColor,
                                    foregroundColor: darkClearTextColor,
                                    side: BorderSide(
                                      color: darkClearBorderColor,
                                      width: 1,
                                    ),
                                    shape: RoundedRectangleBorder(
                                      borderRadius: BorderRadius.circular(16),
                                    ),
                                  ),
                                  child: const Text('重置'),
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 16),

                      // 选项
                      const Text('下载选项:'),
                      const SizedBox(height: 8),

                      CheckboxListTile(
                        title: const Text('不再提示保存位置'),
                        subtitle: const Text('下次直接保存到此位置'),
                        value: tempSkipLocationPrompt,
                        onChanged: (value) {
                          setState(() {
                            tempSkipLocationPrompt = value ?? false;
                          });
                        },
                        dense: true,
                        contentPadding: EdgeInsets.zero,
                      ),
                    ],
                  ),
                ),
                actions: [
                  TextButton(
                    onPressed:
                        () => Navigator.of(context).pop({'action': 'cancel'}),
                    child: const Text('取消'),
                  ),
                  Container(
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
                    child: ElevatedButton(
                      onPressed: () {
                        Navigator.of(context).pop({
                          'action': 'save',
                          'skipLocationPrompt': tempSkipLocationPrompt,
                        });
                      },
                      style: ElevatedButton.styleFrom(
                        padding: const EdgeInsets.symmetric(
                          vertical: 12,
                          horizontal: 16,
                        ),
                        backgroundColor: primaryColor,
                        foregroundColor:
                            isDarkMode ? Colors.white : onPrimaryColor,
                        side: BorderSide.none,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(16),
                        ),
                        elevation: 0,
                      ),
                      child: const Text('保存'),
                    ),
                  ),
                ],
              );
            },
          );
        },
      );

      if (result == null) {
        return;
      }

      final action = result['action'] as String;

      if (action == 'cancel') {
        return;
      }

      if (action == 'save') {
        // 保存设置
        await DownloadService.setSkipLocationPrompt(
          result['skipLocationPrompt'] as bool,
        );

        BotToast.showText(
          text: '设置已保存',
          duration: const Duration(seconds: 2),
          contentColor: Colors.black54,
          textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
        );
        return;
      }

      if (action == 'change_location') {
        // 更改位置
        final newDir = await FilePicker.platform.getDirectoryPath(
          dialogTitle: '选择保存位置',
        );
        if (newDir != null && newDir.isNotEmpty) {
          await DownloadService.setSaveLocation(newDir);
          BotToast.showText(
            text: '保存位置已更新',
            duration: const Duration(seconds: 2),
            contentColor: Colors.green,
          );
          // 重新打开设置对话框
          Future.delayed(const Duration(milliseconds: 300), () {
            if (context.mounted) {
              _showSaveLocationSettings(context);
            }
          });
        } else {
          // 用户取消了选择，重新打开原对话框
          Future.delayed(const Duration(milliseconds: 300), () {
            if (context.mounted) {
              _showSaveLocationSettings(context);
            }
          });
        }
        return;
      }

      if (action == 'reset') {
        // 显示重置确认对话框
        final confirmed = await showDialog<bool>(
          context: context,
          builder:
              (context) => AlertDialog(
                title: const Text('确认重置'),
                content: const Text('确定要重置所有保存设置吗？这将清除保存位置和所有选项。'),
                actions: [
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(false),
                    child: const Text('取消'),
                  ),
                  Container(
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
                    child: ElevatedButton(
                      onPressed: () => Navigator.of(context).pop(true),
                      style: ElevatedButton.styleFrom(
                        padding: const EdgeInsets.symmetric(
                          vertical: 12,
                          horizontal: 16,
                        ),
                        backgroundColor: primaryColor,
                        foregroundColor:
                            isDarkMode ? Colors.white : onPrimaryColor,
                        side: BorderSide.none,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(16),
                        ),
                        elevation: 0,
                      ),
                      child: const Text('重置'),
                    ),
                  ),
                ],
              ),
        );

        if (confirmed == true) {
          await DownloadService.resetSaveLocationSettings();
          BotToast.showText(
            text: '保存设置已重置',
            duration: const Duration(seconds: 2),
            contentColor: Colors.black54,
            textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
          );
          // 重新打开设置对话框
          Future.delayed(const Duration(milliseconds: 300), () {
            if (context.mounted) {
              _showSaveLocationSettings(context);
            }
          });
        } else {
          // 用户取消了重置，重新打开原对话框
          Future.delayed(const Duration(milliseconds: 300), () {
            if (context.mounted) {
              _showSaveLocationSettings(context);
            }
          });
        }
        return;
      }
    } catch (e) {
      print('显示保存位置设置失败: $e');
      BotToast.showText(
        text: '设置失败: ${e.toString()}',
        duration: const Duration(seconds: 3),
        contentColor: Colors.red,
      );
    }
  }

  // 关于对话框
  void _showAboutDialog(BuildContext context) {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('关于'),
          content: const SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  '抖音视频解析工具',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
                ),
                SizedBox(height: 8),
                Text('版本: 20260110'),
                SizedBox(height: 8),
                Text('功能: 解析抖音视频链接，获取视频下载地址'),
                SizedBox(height: 8),
                Text('注意: 仅用于学习和研究目的'),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('确定'),
            ),
          ],
        );
      },
    );
  }
}
