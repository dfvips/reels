import 'dart:io';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

class AppSettings extends ChangeNotifier {
  ThemeMode _themeMode = ThemeMode.system;
  String _themeName = '跟随系统';
  String _cacheSize = '计算中...';

  ThemeMode get themeMode => _themeMode;
  String get themeName => _themeName;
  String get cacheSize => _cacheSize;

  AppSettings() {
    _loadTheme();
    _calculateCacheSize();
  }

  Future<void> _loadTheme() async {
    final prefs = await SharedPreferences.getInstance();
    final themeIndex = prefs.getInt('theme_mode') ?? 0;

    switch (themeIndex) {
      case 0:
        _themeMode = ThemeMode.system;
        _themeName = '跟随系统';
        break;
      case 1:
        _themeMode = ThemeMode.light;
        _themeName = '浅色模式';
        break;
      case 2:
        _themeMode = ThemeMode.dark;
        _themeName = '深色模式';
        break;
    }
    notifyListeners();
  }

  Future<void> _saveTheme(int index) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt('theme_mode', index);
  }

  void setThemeMode(ThemeMode mode, String name) {
    _themeMode = mode;
    _themeName = name;

    int index = 0;
    switch (mode) {
      case ThemeMode.system:
        index = 0;
        break;
      case ThemeMode.light:
        index = 1;
        break;
      case ThemeMode.dark:
        index = 2;
        break;
    }

    _saveTheme(index);
    notifyListeners();
  }

  // 计算缓存大小
  Future<void> _calculateCacheSize() async {
    try {
      int totalSize = 0;
      List<String> cachePaths = [];

      // 获取所有可能的缓存目录
      if (Platform.isAndroid || Platform.isIOS) {
        // 移动端
        final appDocDir = await getApplicationDocumentsDirectory();
        final tempDir = await getTemporaryDirectory();
        cachePaths = [appDocDir.path, tempDir.path];

        // 如果是Android，还有外部存储目录
        if (Platform.isAndroid) {
          final externalDir = await getExternalStorageDirectory();
          if (externalDir != null) {
            cachePaths.add(externalDir.path);
          }
        }
      } else if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
        // 桌面端
        final appDocDir = await getApplicationDocumentsDirectory();
        final tempDir = await getTemporaryDirectory();
        final downloadDir = await getDownloadsDirectory();

        cachePaths = [appDocDir.path, tempDir.path];
        if (downloadDir != null) {
          cachePaths.add(downloadDir.path);
        }
      }

      print('缓存检查目录:');
      for (String path in cachePaths) {
        print('- $path');
      }

      // 计算总缓存大小
      for (String cachePath in cachePaths) {
        try {
          final dir = Directory(cachePath);
          if (await dir.exists()) {
            totalSize += await _calculateDirectorySize(dir);
          }
        } catch (e) {
          print('计算目录大小失败: $cachePath, 错误: $e');
        }
      }

      // 格式化显示
      if (totalSize < 1024) {
        _cacheSize = '$totalSize B';
      } else if (totalSize < 1024 * 1024) {
        _cacheSize = '${(totalSize / 1024).toStringAsFixed(1)} KB';
      } else if (totalSize < 1024 * 1024 * 1024) {
        _cacheSize = '${(totalSize / (1024 * 1024)).toStringAsFixed(1)} MB';
      } else {
        _cacheSize =
            '${(totalSize / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
      }

      notifyListeners();
      print('总缓存大小: $_cacheSize');
    } catch (e) {
      print('计算缓存大小失败: $e');
      _cacheSize = '计算失败';
      notifyListeners();
    }
  }

  // 递归计算目录大小
  Future<int> _calculateDirectorySize(Directory dir) async {
    int size = 0;
    try {
      final List<FileSystemEntity> entities = await dir.list().toList();

      for (FileSystemEntity entity in entities) {
        if (entity is File) {
          try {
            size += await entity.length();
          } catch (e) {
            print('无法获取文件大小: ${entity.path}, 错误: $e');
          }
        } else if (entity is Directory) {
          try {
            size += await _calculateDirectorySize(entity);
          } catch (e) {
            print('无法计算目录大小: ${entity.path}, 错误: $e');
          }
        }
      }
    } catch (e) {
      print('遍历目录失败: ${dir.path}, 错误: $e');
    }
    return size;
  }

  // 清理缓存
  Future<void> clearCache() async {
    try {
      List<String> cachePaths = [];

      // 获取所有可能的缓存目录
      if (Platform.isAndroid || Platform.isIOS) {
        // 移动端
        final appDocDir = await getApplicationDocumentsDirectory();
        final tempDir = await getTemporaryDirectory();
        cachePaths = [appDocDir.path, tempDir.path];

        // 如果是Android，还有外部存储目录
        if (Platform.isAndroid) {
          final externalDir = await getExternalStorageDirectory();
          if (externalDir != null) {
            cachePaths.add(externalDir.path);
          }
        }
      } else if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
        // 桌面端
        final appDocDir = await getApplicationDocumentsDirectory();
        final tempDir = await getTemporaryDirectory();
        final downloadDir = await getDownloadsDirectory();

        cachePaths = [appDocDir.path, tempDir.path];
        if (downloadDir != null) {
          cachePaths.add(downloadDir.path);
        }
      }

      int deletedCount = 0;
      int deletedSize = 0;

      print('开始清理缓存，目录列表:');
      for (String path in cachePaths) {
        print('- $path');
      }

      for (String cachePath in cachePaths) {
        try {
          final dir = Directory(cachePath);
          if (await dir.exists()) {
            final result = await _cleanDirectory(dir);
            deletedCount += result['count']!;
            deletedSize += result['size']!;
          }
        } catch (e) {
          print('清理目录失败: $cachePath, 错误: $e');
        }
      }

      print('清理完成: 删除了 $deletedCount 个文件，总大小 ${_formatSize(deletedSize)}');

      // 重新计算缓存大小
      await _calculateCacheSize();
    } catch (e) {
      print('清理缓存失败: $e');
    }
  }

  // 清理目录中的临时文件和缓存文件
  Future<Map<String, int>> _cleanDirectory(Directory dir) async {
    int deletedCount = 0;
    int deletedSize = 0;

    try {
      final List<FileSystemEntity> entities = await dir.list().toList();

      for (FileSystemEntity entity in entities) {
        try {
          if (entity is File) {
            // 检查是否为临时文件或缓存文件
            final fileName = entity.uri.pathSegments.last.toLowerCase();
            if (fileName.contains('.tmp') ||
                fileName.contains('.temp') ||
                fileName.contains('.cache') ||
                fileName.contains('douyin')) {
              try {
                final fileSize = await entity.length();
                await entity.delete();
                deletedCount++;
                deletedSize += fileSize;
                print('已删除文件: ${entity.path}, 大小: ${_formatSize(fileSize)}');
              } catch (e) {
                print('删除文件失败: ${entity.path}, 错误: $e');
              }
            }
          } else if (entity is Directory) {
            // 检查是否为抖音相关目录
            final dirName = entity.uri.pathSegments.last.toLowerCase();
            if (dirName.contains('douyin') ||
                dirName.contains('cache') ||
                dirName.contains('temp')) {
              try {
                final result = await _cleanDirectory(entity);
                deletedCount += result['count']!;
                deletedSize += result['size']!;

                // 删除空目录
                final remaining = await entity.list().toList();
                if (remaining.isEmpty) {
                  await entity.delete();
                  print('已删除空目录: ${entity.path}');
                }
              } catch (e) {
                print('清理子目录失败: ${entity.path}, 错误: $e');
              }
            }
          }
        } catch (e) {
          print('处理文件系统实体失败: ${entity.path}, 错误: $e');
        }
      }
    } catch (e) {
      print('遍历清理目录失败: ${dir.path}, 错误: $e');
    }

    return {'count': deletedCount, 'size': deletedSize};
  }

  // 格式化文件大小
  String _formatSize(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    if (bytes < 1024 * 1024 * 1024)
      return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
    return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
  }

  // 刷新缓存大小显示
  Future<void> refreshCacheSize() async {
    await _calculateCacheSize();
  }
}
