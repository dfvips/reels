import 'dart:io';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/widgets/setting_item.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:file_picker/file_picker.dart';
import 'package:bot_toast/bot_toast.dart';

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

  // 使用 BuildContext 的静态方法
  static Future<void> showSaveLocationSettings(BuildContext context) async {
    await _showSaveLocationSettings(context);
  }

  @override
  Widget build(BuildContext context) {
    final settings = Provider.of<AppSettings>(context);

    return Scaffold(
      appBar: AppBar(title: const Text('设置'), centerTitle: true),
      body: ListView(
        children: [
          const SizedBox(height: 8),

          // 保存位置设置（仅桌面端显示）
          if (Platform.isMacOS || Platform.isWindows || Platform.isLinux)
            Column(
              children: [
                SettingItem(
                  title: '保存位置',
                  subtitle: '设置视频保存的默认位置',
                  icon: Icons.folder,
                  onTap: () {
                    _showSaveLocationSettings(context);
                  },
                ),
                const Divider(height: 1),
              ],
            ),

          // 主题设置 - 修改为可点击整个项
          Material(
            color: Theme.of(context).colorScheme.surface,
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
                          const Text(
                            '主题设置',
                            style: TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.w500,
                            ),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            '当前: ${settings.themeName}',
                            style: TextStyle(
                              fontSize: 14,
                              color:
                                  Theme.of(
                                    context,
                                  ).colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    Icon(
                      Icons.chevron_right,
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ],
                ),
              ),
            ),
          ),
          const Divider(height: 1),

          // 关于
          SettingItem(
            title: '关于',
            subtitle: '应用版本: 20260106',
            icon: Icons.info_outline,
            onTap: () {
              _showAboutDialog(context);
            },
          ),
          const Divider(height: 1),

          // 清除缓存 - 显示缓存大小和位置
          Material(
            color: Theme.of(context).colorScheme.surface,
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
                          const Text(
                            '清除缓存',
                            style: TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.w500,
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
                                          Theme.of(
                                            context,
                                          ).colorScheme.onSurfaceVariant,
                                    ),
                                  ),
                                  if (settings.cachePaths.isNotEmpty)
                                    Text(
                                      '${settings.cachePaths.length}个位置',
                                      style: TextStyle(
                                        fontSize: 12,
                                        color: Theme.of(context)
                                            .colorScheme
                                            .onSurfaceVariant
                                            .withOpacity(0.7),
                                      ),
                                    ),
                                ],
                              );
                            },
                          ),
                        ],
                      ),
                    ),
                    Icon(
                      Icons.chevron_right,
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
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
            ElevatedButton(
              onPressed: () async {
                Navigator.pop(context);

                // 显示清理中的提示
                final cancel = BotToast.showLoading();

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
              child: const Text('清理'),
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
      final skipFilenameCustom =
          await DownloadService.getSkipFilenameCustomization();

      bool tempSkipLocationPrompt = skipLocationPrompt;
      bool tempSkipFilenameCustom = skipFilenameCustom;

      final result = await showDialog<Map<String, dynamic>>(
        context: context,
        builder: (context) {
          return StatefulBuilder(
            builder: (context, setState) {
              return AlertDialog(
                title: const Text('保存位置设置'),
                content: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // 当前保存位置
                      const Text('当前保存位置:'),
                      const SizedBox(height: 4),
                      Container(
                        padding: const EdgeInsets.all(8),
                        decoration: BoxDecoration(
                          color: Colors.grey[100],
                          borderRadius: BorderRadius.circular(4),
                        ),
                        child: Text(
                          currentLocation ?? '未设置',
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      const SizedBox(height: 12),

                      // 更改位置按钮
                      OutlinedButton.icon(
                        icon: const Icon(Icons.folder_open, size: 16),
                        label: const Text('更改保存位置'),
                        onPressed: () async {
                          Navigator.of(
                            context,
                          ).pop({'action': 'change_location'});
                        },
                      ),
                      const SizedBox(height: 16),

                      // 重置按钮
                      OutlinedButton.icon(
                        icon: const Icon(Icons.restart_alt, size: 16),
                        label: const Text('重置保存设置'),
                        onPressed: () async {
                          Navigator.of(context).pop({'action': 'reset'});
                        },
                      ),
                      const SizedBox(height: 16),

                      // 选项
                      const Text('下载选项:'),
                      const SizedBox(height: 8),

                      CheckboxListTile(
                        title: const Text('不再提示保存位置'),
                        subtitle: const Text('下次直接使用当前设置的位置保存'),
                        value: tempSkipLocationPrompt,
                        onChanged: (value) {
                          setState(() {
                            tempSkipLocationPrompt = value ?? false;
                          });
                        },
                        dense: true,
                        contentPadding: EdgeInsets.zero,
                      ),
                      CheckboxListTile(
                        title: const Text('不再提示文件名自定义'),
                        subtitle: const Text('下次直接保存，不再弹出文件名输入窗口'),
                        value: tempSkipFilenameCustom,
                        onChanged: (value) {
                          setState(() {
                            tempSkipFilenameCustom = value ?? false;
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
                  ElevatedButton(
                    onPressed: () {
                      Navigator.of(context).pop({
                        'action': 'save',
                        'skipLocationPrompt': tempSkipLocationPrompt,
                        'skipFilenameCustom': tempSkipFilenameCustom,
                      });
                    },
                    child: const Text('保存'),
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
        await DownloadService.setSkipFilenameCustomization(
          result['skipFilenameCustom'] as bool,
        );

        BotToast.showText(
          text: '设置已保存',
          duration: const Duration(seconds: 2),
          contentColor: Colors.green,
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
                  ElevatedButton(
                    onPressed: () => Navigator.of(context).pop(true),
                    child: const Text('重置'),
                  ),
                ],
              ),
        );

        if (confirmed == true) {
          await DownloadService.resetSaveLocationSettings();
          BotToast.showText(
            text: '保存设置已重置',
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
                Text('版本: 20260106'),
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
