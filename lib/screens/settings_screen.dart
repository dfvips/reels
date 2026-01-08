import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/widgets/setting_item.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final settings = Provider.of<AppSettings>(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('设置'),
        centerTitle: true,
      ),
      body: ListView(
        children: [
          const SizedBox(height: 8),
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
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('缓存已清除')),
                );
              },
              child: const Text('清除'),
            ),
          ],
        );
      },
    );
  }
}
