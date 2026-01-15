import 'dart:io';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:path/path.dart' as path;

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
    try {
      final cacheDirPath =
          await DownloadService.getDownloadCacheDirectoryPath();
      _cachePaths.add('下载缓存目录: $cacheDirPath');
    } catch (e) {
      print('获取下载缓存目录失败: $e');
    }

    if (Platform.isIOS) {
      try {
        final docDir = await getApplicationDocumentsDirectory();
        _cachePaths.add('旧下载位置(iOS文档): ${docDir.path}');
      } catch (e) {
        print('获取iOS文档目录失败: $e');
      }
    }

    if (Platform.isAndroid) {
      try {
        final externalDir = await getExternalStorageDirectory();
        if (externalDir != null) {
          final legacyFolder = Directory('${externalDir.path}/DouyinVideos');
          _cachePaths.add('旧下载位置(Android): ${legacyFolder.path}');
        }
      } catch (e) {
        print('获取Android旧下载目录失败: $e');
      }
    }
  }

  // 计算缓存大小
  Future<void> _calculateCacheSize() async {
    try {
      await _getCachePaths();

      int totalSize = 0;

      final cacheDirPath =
          await DownloadService.getDownloadCacheDirectoryPath();
      final cacheDir = Directory(cacheDirPath);
      if (await cacheDir.exists()) {
        totalSize += await _calculateDirectorySize(cacheDir);
      }

      if (Platform.isIOS) {
        try {
          final docDir = await getApplicationDocumentsDirectory();
          if (await docDir.exists()) {
            final entities = await docDir.list(followLinks: false).toList();
            for (final entity in entities) {
              if (entity is File &&
                  entity.path.toLowerCase().endsWith('.mp4')) {
                try {
                  totalSize += await entity.length();
                } catch (_) {}
              }
              if (entity is Directory &&
                  entity.path.toLowerCase().endsWith('/douyinvideos')) {
                totalSize += await _calculateDirectorySize(entity);
              }
            }
          }
        } catch (e) {
          print('计算iOS旧下载大小失败: $e');
        }
      }

      if (Platform.isAndroid) {
        try {
          final externalDir = await getExternalStorageDirectory();
          if (externalDir != null) {
            final legacyFolder = Directory('${externalDir.path}/DouyinVideos');
            if (await legacyFolder.exists()) {
              totalSize += await _calculateDirectorySize(legacyFolder);
            }
          }
        } catch (e) {
          print('计算Android旧下载大小失败: $e');
        }
      }

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
  Future<Map<String, dynamic>> clearCache() async {
    _cleanedPaths.clear();
    int deletedCount = 0;
    int deletedSize = 0;
    List<String> cleanedDetails = [];

    try {
      await _getCachePaths();

      print('=== 开始清理缓存 ===');

      try {
        final cacheDirPath =
            await DownloadService.getDownloadCacheDirectoryPath();
        final cacheDir = Directory(cacheDirPath);
        if (await cacheDir.exists()) {
          final beforeSize = await _calculateDirectorySize(cacheDir);
          await DownloadService.clearDownloadCache();
          final detail =
              '🗑️ [下载缓存目录] 已清理: $cacheDirPath (释放 ${_formatSize(beforeSize)})';
          cleanedDetails.add(detail);
          deletedSize += beforeSize;
        }
      } catch (e) {
        cleanedDetails.add('❌ [下载缓存目录] 清理失败: $e');
      }

      if (Platform.isIOS) {
        try {
          final docDir = await getApplicationDocumentsDirectory();
          if (await docDir.exists()) {
            final entities = await docDir.list(followLinks: false).toList();
            for (final entity in entities) {
              if (entity is File &&
                  entity.path.toLowerCase().endsWith('.mp4')) {
                try {
                  final size = await entity.length();
                  await entity.delete();
                  deletedCount++;
                  deletedSize += size;
                  cleanedDetails.add(
                    '🗑️ [旧下载位置(iOS文档)] 已删除文件: ${path.basename(entity.path)} (${_formatSize(size)})',
                  );
                } catch (_) {}
              }
              if (entity is Directory &&
                  entity.path.toLowerCase().endsWith('/douyinvideos')) {
                try {
                  final size = await _calculateDirectorySize(entity);
                  await entity.delete(recursive: true);
                  deletedSize += size;
                  cleanedDetails.add(
                    '🗑️ [旧下载位置(iOS文档)] 已删除目录: ${path.basename(entity.path)} (${_formatSize(size)})',
                  );
                } catch (_) {}
              }
            }
          }
        } catch (e) {
          cleanedDetails.add('❌ [旧下载位置(iOS文档)] 清理失败: $e');
        }
      }

      if (Platform.isAndroid) {
        try {
          final externalDir = await getExternalStorageDirectory();
          if (externalDir != null) {
            final legacyFolder = Directory('${externalDir.path}/DouyinVideos');
            if (await legacyFolder.exists()) {
              final size = await _calculateDirectorySize(legacyFolder);
              await legacyFolder.delete(recursive: true);
              deletedSize += size;
              cleanedDetails.add(
                '🗑️ [旧下载位置(Android)] 已删除目录: ${legacyFolder.path} (${_formatSize(size)})',
              );
            }
          }
        } catch (e) {
          cleanedDetails.add('❌ [旧下载位置(Android)] 清理失败: $e');
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
          } else if (entity is Directory) {
            final subDirName = entity.uri.pathSegments.last.toLowerCase();
            try {
              final result = await _cleanDirectory(
                entity,
                '$dirName/$subDirName',
              );
              deletedCount += (result['count'] as int);
              deletedSize += (result['size'] as int);
              cleanedDetails.addAll(result['details']);

              try {
                await entity.delete(recursive: true);
                final detail = '🗑️ [$dirName] 已删除目录: $subDirName';
                cleanedDetails.add(detail);
                print(detail);
              } catch (_) {}
            } catch (e) {
              final detail = '❌ [$dirName] 清理子目录失败: $subDirName, 错误: $e';
              cleanedDetails.add(detail);
              print(detail);
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
