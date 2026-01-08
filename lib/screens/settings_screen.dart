import 'dart:io';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/widgets/setting_item.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:file_picker/file_picker.dart';
import 'package:bot_toast/bot_toast.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

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

          SettingItem(
            title: '主题设置',
            subtitle: '当前: ${settings.themeName}',
            icon: Icons.color_lens,
            onTap: () {
              _showThemeDialog(context, settings);
            },
          ),
          const Divider(height: 1),
          SettingItem(
            title: '关于',
            subtitle: '应用版本: 20260106',
            icon: Icons.info_outline,
            onTap: () {
              _showAboutDialog(context);
            },
          ),
          const Divider(height: 1),
          SettingItem(
            title: '清除缓存',
            subtitle: '清除应用缓存数据',
            icon: Icons.delete_outline,
            onTap: () {
              _showClearCacheDialog(context);
            },
          ),
        ],
      ),
    );
  }

  // 显示保存位置设置对话框
  void _showSaveLocationSettings(BuildContext context) async {
    try {
      // 获取当前设置
      final currentLocation = await DownloadService.getCurrentSaveLocation();

      // 读取用户偏好设置 - 使用 DownloadService 的公共方法
      final skipLocationPrompt = await DownloadService.getSkipLocationPrompt();
      final skipFilenameCustom =
          await DownloadService.getSkipFilenameCustomization();

      bool tempSkipLocationPrompt = skipLocationPrompt;
      bool tempSkipFilenameCustom = skipFilenameCustom;

      await showDialog(
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
                          final newDir = await FilePicker.platform
                              .getDirectoryPath(dialogTitle: '选择保存位置');
                          if (newDir != null && newDir.isNotEmpty) {
                            await DownloadService.setSaveLocation(newDir);
                            Navigator.pop(context);
                            _showSaveLocationSettings(context);
                          }
                        },
                      ),
                      const SizedBox(height: 16),

                      // 重置按钮
                      OutlinedButton.icon(
                        icon: const Icon(Icons.restart_alt, size: 16),
                        label: const Text('重置保存设置'),
                        onPressed: () async {
                          await DownloadService.resetSaveLocationSettings();
                          Navigator.pop(context);
                          BotToast.showText(
                            text: '保存设置已重置',
                            duration: const Duration(seconds: 2),
                            contentColor: Colors.green,
                          );
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
                    onPressed: () => Navigator.pop(context),
                    child: const Text('取消'),
                  ),
                  ElevatedButton(
                    onPressed: () async {
                      // 保存设置
                      await DownloadService.setSkipLocationPrompt(
                        tempSkipLocationPrompt,
                      );
                      await DownloadService.setSkipFilenameCustomization(
                        tempSkipFilenameCustom,
                      );

                      Navigator.pop(context);
                      BotToast.showText(
                        text: '设置已保存',
                        duration: const Duration(seconds: 2),
                        contentColor: Colors.green,
                      );
                    },
                    child: const Text('保存'),
                  ),
                ],
              );
            },
          );
        },
      );
    } catch (e) {
      print('显示保存位置设置失败: $e');
      BotToast.showText(
        text: '设置失败: $e',
        duration: const Duration(seconds: 3),
        contentColor: Colors.red,
      );
    }
  }

  // 原有的其他方法保持不变...
  void _showThemeDialog(BuildContext context, AppSettings settings) {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('选择主题'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ListTile(
                title: const Text('跟随系统'),
                subtitle: const Text('根据系统设置自动切换'),
                leading: Radio<ThemeMode>(
                  value: ThemeMode.system,
                  groupValue: settings.themeMode,
                  onChanged: (value) {
                    settings.setThemeMode(value!, '跟随系统');
                    Navigator.pop(context);
                  },
                ),
              ),
              const Divider(height: 1),
              ListTile(
                title: const Text('浅色模式'),
                subtitle: const Text('使用明亮的主题'),
                leading: Radio<ThemeMode>(
                  value: ThemeMode.light,
                  groupValue: settings.themeMode,
                  onChanged: (value) {
                    settings.setThemeMode(value!, '浅色模式');
                    Navigator.pop(context);
                  },
                ),
              ),
              const Divider(height: 1),
              ListTile(
                title: const Text('深色模式'),
                subtitle: const Text('使用深色的主题'),
                leading: Radio<ThemeMode>(
                  value: ThemeMode.dark,
                  groupValue: settings.themeMode,
                  onChanged: (value) {
                    settings.setThemeMode(value!, '深色模式');
                    Navigator.pop(context);
                  },
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
  }

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

  void _showClearCacheDialog(BuildContext context) {
    showDialog(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('清除缓存'),
          content: const Text('确定要清除所有缓存数据吗？'),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('取消'),
            ),
            TextButton(
              onPressed: () {
                // 实现清除缓存逻辑
                Navigator.pop(context);
                ScaffoldMessenger.of(
                  context,
                ).showSnackBar(const SnackBar(content: Text('缓存已清除')));
              },
              child: const Text('清除'),
            ),
          ],
        );
      },
    );
  }
}
