import 'dart:io';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

class AppSettings extends ChangeNotifier {
  ThemeMode _themeMode = ThemeMode.system;
  String _themeName = '跟随系统';
  String _cacheSize = '计算中...';
  List<String> _cachePaths = [];
  List<String> _cleanedPaths = [];

  ThemeMode get themeMode => _themeMode;
  String get themeName => _themeName;
  String get cacheSize => _cacheSize;
  List<String> get cachePaths => _cachePaths;
  List<String> get cleanedPaths => _cleanedPaths;

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

  // 获取缓存目录
  Future<void> _getCachePaths() async {
    _cachePaths.clear();

    if (Platform.isAndroid || Platform.isIOS) {
      // 移动端
      try {
        final appDocDir = await getApplicationDocumentsDirectory();
        _cachePaths.add('文档目录: ${appDocDir.path}');
      } catch (e) {
        print('获取应用文档目录失败: $e');
      }

      try {
        final tempDir = await getTemporaryDirectory();
        _cachePaths.add('临时目录: ${tempDir.path}');
      } catch (e) {
        print('获取临时目录失败: $e');
      }

      if (Platform.isAndroid) {
        try {
          final externalDir = await getExternalStorageDirectory();
          if (externalDir != null) {
            _cachePaths.add('外部存储: ${externalDir.path}');
          }
        } catch (e) {
          print('获取外部存储目录失败: $e');
        }
      }
    } else if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
      // 桌面端
      try {
        final appDocDir = await getApplicationDocumentsDirectory();
        _cachePaths.add('文档目录: ${appDocDir.path}');
      } catch (e) {
        print('获取应用文档目录失败: $e');
      }

      try {
        final tempDir = await getTemporaryDirectory();
        _cachePaths.add('临时目录: ${tempDir.path}');
      } catch (e) {
        print('获取临时目录失败: $e');
      }
    }
  }

  // 计算缓存大小
  Future<void> _calculateCacheSize() async {
    try {
      int totalSize = 0;
      await _getCachePaths();

      print('=== 缓存检查目录 ===');
      for (String cachePath in _cachePaths) {
        print(cachePath);
      }
      print('===================');

      // 计算总缓存大小
      for (String cachePathInfo in _cachePaths) {
        try {
          // 从字符串中提取实际路径
          final path = cachePathInfo.split(': ')[1];
          final dir = Directory(path);
          if (await dir.exists()) {
            totalSize += await _calculateDirectorySize(dir);
          }
        } catch (e) {
          print('计算目录大小失败: $cachePathInfo, 错误: $e');
        }
      }

      // 格式化显示
      _cacheSize = _formatSize(totalSize);

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
          // 检查是否为需要扫描的子目录
          final dirName = entity.uri.pathSegments.last.toLowerCase();
          if (dirName.contains('douyin') ||
              dirName.contains('cache') ||
              dirName.contains('temp')) {
            try {
              size += await _calculateDirectorySize(entity);
            } catch (e) {
              print('无法计算目录大小: ${entity.path}, 错误: $e');
            }
          }
        }
      }
    } catch (e) {
      print('遍历目录失败: ${dir.path}, 错误: $e');
    }
    return size;
  }

  // 清理缓存
  Future<Map<String, dynamic>> clearCache() async {
    _cleanedPaths.clear();
    int deletedCount = 0;
    int deletedSize = 0;
    List<String> cleanedDetails = [];

    try {
      await _getCachePaths();

      print('=== 开始清理缓存 ===');
      for (String cachePathInfo in _cachePaths) {
        print('检查目录: $cachePathInfo');
      }

      for (String cachePathInfo in _cachePaths) {
        try {
          final path = cachePathInfo.split(': ')[1];
          final dir = Directory(path);
          if (await dir.exists()) {
            final result = await _cleanDirectory(
              dir,
              cachePathInfo.split(': ')[0],
            );
            deletedCount += (result['count'] as int);
            deletedSize += (result['size'] as int);
            cleanedDetails.addAll(result['details']);
          }
        } catch (e) {
          cleanedDetails.add('❌ ${cachePathInfo.split(': ')[0]}清理失败: $e');
          print('清理目录失败: $cachePathInfo, 错误: $e');
        }
      }

      print('=== 清理完成 ===');
      print('删除了 $deletedCount 个文件，总大小 ${_formatSize(deletedSize)}');

      // 保存清理的路径信息
      _cleanedPaths = cleanedDetails;

      // 重新计算缓存大小
      await _calculateCacheSize();

      notifyListeners();

      return {
        'success': true,
        'deletedCount': deletedCount,
        'deletedSize': deletedSize,
        'cleanedPaths': cleanedDetails,
        'message':
            '清理完成: 删除了 $deletedCount 个文件，释放 ${_formatSize(deletedSize)} 空间',
      };
    } catch (e) {
      print('清理缓存失败: $e');
      return {
        'success': false,
        'message': '清理缓存失败: $e',
        'cleanedPaths': cleanedDetails,
      };
    }
  }

  // 清理目录中的临时文件和缓存文件
  Future<Map<String, dynamic>> _cleanDirectory(
    Directory dir,
    String dirName,
  ) async {
    int deletedCount = 0;
    int deletedSize = 0;
    List<String> cleanedDetails = [];

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
                fileName.contains('douyin_') ||
                fileName.endsWith('.mp4') && fileName.contains('douyin')) {
              try {
                final fileSize = await entity.length();
                await entity.delete();
                deletedCount++;
                deletedSize += fileSize;

                final detail =
                    '🗑️ [$dirName] 已删除文件: ${entity.uri.pathSegments.last} (${_formatSize(fileSize)})';
                cleanedDetails.add(detail);
                print(detail);
              } catch (e) {
                final detail =
                    '❌ [$dirName] 删除文件失败: ${entity.uri.pathSegments.last}, 错误: $e';
                cleanedDetails.add(detail);
                print(detail);
              }
            }
          } else if (entity is Directory) {
            // 检查是否为抖音相关目录
            final subDirName = entity.uri.pathSegments.last.toLowerCase();
            if (subDirName.contains('douyin') ||
                subDirName.contains('cache') ||
                subDirName.contains('temp') ||
                subDirName.contains('dart_tool')) {
              try {
                final result = await _cleanDirectory(
                  entity,
                  '$dirName/$subDirName',
                );
                deletedCount += (result['count'] as int);
                deletedSize += (result['size'] as int);
                cleanedDetails.addAll(result['details']);

                // 删除空目录
                final remaining = await entity.list().toList();
                if (remaining.isEmpty) {
                  await entity.delete();
                  final detail = '🗑️ [$dirName] 已删除空目录: $subDirName';
                  cleanedDetails.add(detail);
                  print(detail);
                }
              } catch (e) {
                final detail = '❌ [$dirName] 清理子目录失败: $subDirName, 错误: $e';
                cleanedDetails.add(detail);
                print(detail);
              }
            }
          }
        } catch (e) {
          print('处理文件系统实体失败: ${entity.path}, 错误: $e');
        }
      }
    } catch (e) {
      final detail = '❌ [$dirName] 遍历目录失败: ${dir.path}, 错误: $e';
      cleanedDetails.add(detail);
      print(detail);
    }

    return {
      'count': deletedCount,
      'size': deletedSize,
      'details': cleanedDetails,
    };
  }

  // 格式化文件大小
  String _formatSize(int bytes) {
    if (bytes <= 0) return "0 B";
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
