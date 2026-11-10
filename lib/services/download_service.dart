import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:bot_toast/bot_toast.dart';
import 'package:file_picker/file_picker.dart';
import 'package:open_file/open_file.dart';
import 'package:device_info_plus/device_info_plus.dart';
import 'package:gal/gal.dart';
import 'package:path/path.dart' as path;
import 'package:shared_preferences/shared_preferences.dart';

class DownloadService {
  static final Dio _dio = Dio();
  static CancelToken? _currentCancelToken;
  static const String _downloadCacheDirName = 'douyin_parser_download_cache';
  static const int _chunkCount = 4;
  static const int _maxChunkRetries = 3;

  // 存储设置的键名
  static const String _rememberLocationKey = 'remember_download_location';
  static const String _skipFilenameCustomKey = 'skip_filename_customization';
  static const String _skipLocationPromptKey = 'skip_location_prompt';
  static const String _lastDirectoryKey = 'last_download_directory';
  static const String _hasMacOSPermissionKey = 'macos_has_directory_permission';

  // 添加 formatFileSize 为公共静态方法
  static String formatFileSize(int bytes) {
    if (bytes <= 0) return "0 B";
    const suffixes = ["B", "KB", "MB", "GB", "TB"];
    final i = (log(bytes) / log(1024)).floor();
    return '${(bytes / pow(1024, i)).toStringAsFixed(1)} ${suffixes[i]}';
  }

  static Future<String> getDownloadCacheDirectoryPath() async {
    return (await _getDownloadCacheDirectory()).path;
  }

  static Future<void> clearDownloadCache() async {
    final dir = await _getDownloadCacheDirectory();
    if (await dir.exists()) {
      await dir.delete(recursive: true);
    }
    await dir.create(recursive: true);
  }

  static Future<Directory> _getDownloadCacheDirectory() async {
    Directory baseDir;
    try {
      baseDir = await getApplicationSupportDirectory();
    } catch (_) {
      baseDir = await getApplicationDocumentsDirectory();
    }
    final dir = Directory(path.join(baseDir.path, _downloadCacheDirName));
    if (!await dir.exists()) {
      await dir.create(recursive: true);
    }
    return dir;
  }

  static Future<Directory> _createDownloadSessionDirectory(
    String filePath,
  ) async {
    final baseDir = await _getDownloadCacheDirectory();
    final safeBaseName = _safeFileBaseName(
      path.basenameWithoutExtension(filePath),
    );
    final sessionId =
        '${DateTime.now().millisecondsSinceEpoch}_${Random().nextInt(1 << 32)}';
    final dir = Directory(
      path.join(baseDir.path, '${safeBaseName}_$sessionId'),
    );
    await dir.create(recursive: true);
    return dir;
  }

  /// 生成跨平台安全的文件基础名（不含扩展名）。
  ///
  /// 抖音标题可能超长且包含非法字符，直接作为文件名会触发
  /// Windows 的路径长度 / 非法名称错误（errno 123），这里统一清洗：
  /// 替换非法与控制字符、压缩空白、去掉结尾的点与空格，并按字符数截断。
  static String safeFileName(
    String input, {
    int maxLength = 100,
    int maxBytes = 200,
  }) {
    var name = input.trim();
    name = name.replaceAll(RegExp(r'[<>:"/\\|?*\x00-\x1F]'), '_');
    name = name.replaceAll(RegExp(r'\s+'), ' ').trim();
    name = name.replaceFirst(RegExp(r'[. ]+$'), '');

    // 同时限制字符数与 UTF-8 字节数：ext4（Android）单个文件名上限
    // 为 255 字节，100 个汉字可达 300 字节，必须按字节截断。
    final buffer = StringBuffer();
    var runeCount = 0;
    var byteCount = 0;
    for (final rune in name.runes) {
      if (runeCount >= maxLength) break;
      final char = String.fromCharCode(rune);
      final charBytes = utf8.encode(char).length;
      if (byteCount + charBytes > maxBytes) break;
      buffer.write(char);
      runeCount++;
      byteCount += charBytes;
    }
    name = buffer.toString().trimRight();

    if (RegExp(
      r'^(con|prn|aux|nul|com[1-9]|lpt[1-9])$',
      caseSensitive: false,
    ).hasMatch(name)) {
      name = '_$name';
    }
    return name.isEmpty ? 'download' : name;
  }

  static String _safeFileBaseName(String input) =>
      safeFileName(input, maxLength: 100);

  static Map<String, String> _buildCommonHeaders({String? range}) {
    final headers = <String, String>{
      'User-Agent':
          Platform.isIOS
              ? 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1'
              : Platform.isAndroid
              ? 'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36'
              : Platform.isWindows
              ? 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
              : 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
      'Referer': 'https://www.douyin.com/',
      'Accept': '*/*',
    };
    if (range != null && range.isNotEmpty) {
      headers['Range'] = range;
    }
    return headers;
  }

  static int? _parseTotalSizeFromContentRange(String? contentRange) {
    if (contentRange == null) return null;
    final match = RegExp(
      r'bytes\s+\d+-\d+/(\d+|\*)',
      caseSensitive: false,
    ).firstMatch(contentRange);
    if (match == null) return null;
    final value = match.group(1);
    if (value == null || value == '*') return null;
    return int.tryParse(value);
  }

  static Future<_RemoteFileInfo> _probeRemoteFileInfo(
    String url,
    CancelToken? cancelToken,
  ) async {
    try {
      final response = await _dio.get<List<int>>(
        url,
        cancelToken: cancelToken,
        options: Options(
          headers: _buildCommonHeaders(range: 'bytes=0-0'),
          followRedirects: true,
          maxRedirects: 5,
          responseType: ResponseType.bytes,
          validateStatus: (status) => true,
          receiveTimeout: const Duration(minutes: 2),
          sendTimeout: const Duration(minutes: 2),
        ),
      );

      final status = response.statusCode ?? 0;
      final contentRange = response.headers.value('content-range');
      final acceptRanges = response.headers.value('accept-ranges');
      final contentLengthHeader = response.headers.value('content-length');
      final contentLength = int.tryParse(contentLengthHeader ?? '');
      final totalSize =
          _parseTotalSizeFromContentRange(contentRange) ??
          (status == 200 ? contentLength : null);

      final supportsRange =
          status == 206 ||
          (acceptRanges != null &&
              acceptRanges.toLowerCase().contains('bytes'));

      return _RemoteFileInfo(
        supportsRange: supportsRange,
        totalSize: totalSize,
      );
    } catch (_) {
      return const _RemoteFileInfo(supportsRange: false, totalSize: null);
    }
  }

  static Future<void> _downloadSingleToPath(
    String url,
    String tempPath,
    CancelToken? cancelToken,
    Function(int received, int total)? progressCallback,
  ) async {
    await _dio.download(
      url,
      tempPath,
      cancelToken: cancelToken,
      onReceiveProgress: (received, total) {
        if (progressCallback != null) {
          progressCallback(received, total > 0 ? total : received * 2);
        }
      },
      options: Options(
        headers: _buildCommonHeaders(),
        receiveTimeout: const Duration(minutes: 10),
        sendTimeout: const Duration(minutes: 10),
        followRedirects: true,
        maxRedirects: 5,
        responseType: ResponseType.bytes,
      ),
    );
  }

  static Future<void> _downloadChunkToPath({
    required String url,
    required int index,
    required int start,
    required int end,
    required String tempPath,
    required CancelToken? cancelToken,
    required List<int> partReceived,
    required int totalSize,
    required Function(int received, int total)? progressCallback,
  }) async {
    final expectedLength = end - start + 1;

    for (var attempt = 1; attempt <= _maxChunkRetries; attempt++) {
      if (cancelToken?.isCancelled == true) {
        throw DioException(
          requestOptions: RequestOptions(path: url),
          type: DioExceptionType.cancel,
        );
      }

      final file = File(tempPath);
      if (await file.exists()) {
        await file.delete();
      }

      await _dio.download(
        url,
        tempPath,
        cancelToken: cancelToken,
        onReceiveProgress: (received, total) {
          partReceived[index] = received;
          if (progressCallback != null) {
            final sum = partReceived.fold<int>(0, (a, b) => a + b);
            progressCallback(sum, totalSize);
          }
        },
        options: Options(
          headers: _buildCommonHeaders(range: 'bytes=$start-$end'),
          followRedirects: true,
          maxRedirects: 5,
          responseType: ResponseType.bytes,
          validateStatus: (status) => true,
          receiveTimeout: const Duration(minutes: 10),
          sendTimeout: const Duration(minutes: 10),
        ),
      );

      final actualLength = await file.length();
      if (actualLength == expectedLength) {
        return;
      }

      if (await file.exists()) {
        await file.delete();
      }
    }

    throw Exception('分块 $index 校验失败，重试后仍不正确');
  }

  static Future<String> _downloadByChunksToTempFile({
    required String url,
    required Directory sessionDir,
    required int totalSize,
    required CancelToken? cancelToken,
    required Function(int received, int total)? progressCallback,
  }) async {
    final partSize = (totalSize / _chunkCount).floor();
    final partReceived = List<int>.filled(_chunkCount, 0);

    final tasks = <Future<void>>[];
    for (var i = 0; i < _chunkCount; i++) {
      final start = i * partSize;
      final end =
          (i == _chunkCount - 1) ? (totalSize - 1) : ((start + partSize) - 1);
      final expectedLength = end - start + 1;
      final partPath = path.join(sessionDir.path, 'part_$i.tmp');

      final partFile = File(partPath);
      if (await partFile.exists()) {
        final len = await partFile.length();
        if (len == expectedLength) {
          partReceived[i] = expectedLength;
          continue;
        } else {
          await partFile.delete();
        }
      }

      tasks.add(
        _downloadChunkToPath(
          url: url,
          index: i,
          start: start,
          end: end,
          tempPath: partPath,
          cancelToken: cancelToken,
          partReceived: partReceived,
          totalSize: totalSize,
          progressCallback: progressCallback,
        ),
      );
    }

    if (progressCallback != null) {
      final sum = partReceived.fold<int>(0, (a, b) => a + b);
      progressCallback(sum, totalSize);
    }

    await Future.wait(tasks);

    final mergedPath = path.join(sessionDir.path, 'merged.tmp');
    final mergedFile = File(mergedPath);
    if (await mergedFile.exists()) {
      await mergedFile.delete();
    }

    final sink = mergedFile.openWrite(mode: FileMode.writeOnly);
    try {
      for (var i = 0; i < _chunkCount; i++) {
        final partPath = path.join(sessionDir.path, 'part_$i.tmp');
        final partFile = File(partPath);
        await sink.addStream(partFile.openRead());
      }
    } finally {
      await sink.close();
    }

    final mergedSize = await mergedFile.length();
    if (mergedSize != totalSize) {
      if (await mergedFile.exists()) {
        await mergedFile.delete();
      }
      throw Exception('合并文件校验失败: $mergedSize != $totalSize');
    }

    return mergedPath;
  }

  static Future<void> _ensureParentDirExists(String filePath) async {
    final dir = Directory(path.dirname(filePath));
    if (!await dir.exists()) {
      await dir.create(recursive: true);
    }
  }

  static Future<void> _replaceFile(String fromPath, String toPath) async {
    final from = File(fromPath);
    final to = File(toPath);

    await _ensureParentDirExists(toPath);
    if (await to.exists()) {
      await to.delete();
    }

    try {
      await from.rename(toPath);
    } catch (_) {
      await from.copy(toPath);
      await from.delete();
    }
  }

  // 初始化默认设置
  static Future<void> initializeDefaultSettings() async {
    final prefs = await SharedPreferences.getInstance();

    // 确保 skipLocationPrompt 默认为 false（显示位置选择）
    if (!prefs.containsKey(_skipLocationPromptKey)) {
      await prefs.setBool(_skipLocationPromptKey, false);
    }

    // 确保 rememberLocation 默认为 false
    if (!prefs.containsKey(_rememberLocationKey)) {
      await prefs.setBool(_rememberLocationKey, false);
    }

    print('默认设置初始化完成');
  }

  // 保存设置到SharedPreferences
  static Future<void> _saveSetting(String key, dynamic value) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      if (value is String) {
        await prefs.setString(key, value);
      } else if (value is bool) {
        await prefs.setBool(key, value);
      } else if (value is int) {
        await prefs.setInt(key, value);
      } else if (value is double) {
        await prefs.setDouble(key, value);
      } else if (value is List<String>) {
        await prefs.setStringList(key, value);
      }
    } catch (e) {
      print('保存设置失败: $e');
    }
  }

  // 从SharedPreferences读取设置
  static Future<dynamic> _getSetting(String key, dynamic defaultValue) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      if (defaultValue is String) {
        return prefs.getString(key) ?? defaultValue;
      } else if (defaultValue is bool) {
        return prefs.getBool(key) ?? defaultValue;
      } else if (defaultValue is int) {
        return prefs.getInt(key) ?? defaultValue;
      } else if (defaultValue is double) {
        return prefs.getDouble(key) ?? defaultValue;
      } else if (defaultValue is List<String>) {
        return prefs.getStringList(key) ?? defaultValue;
      }
    } catch (e) {
      print('读取设置失败: $e');
    }
    return defaultValue;
  }

  // 检查目录是否有写入权限
  static Future<bool> checkDirectoryAccess(String directoryPath) async {
    try {
      final dir = Directory(directoryPath);
      if (!await dir.exists()) {
        print('目录不存在: $directoryPath');
        return false;
      }

      // 尝试在目录中创建一个临时文件来测试权限
      final tempFilePath = path.join(
        directoryPath,
        '.temp_test_${DateTime.now().millisecondsSinceEpoch}.tmp',
      );
      final tempFile = File(tempFilePath);

      try {
        // 尝试写入
        await tempFile.writeAsString('test', flush: true);

        // 尝试读取
        await tempFile.readAsString();

        // 清理临时文件
        if (await tempFile.exists()) {
          await tempFile.delete();
        }

        print('目录权限检查通过: $directoryPath');
        return true;
      } catch (e) {
        print('目录权限检查失败: $e');
        // 清理可能创建的临时文件
        if (await tempFile.exists()) {
          try {
            await tempFile.delete();
          } catch (_) {}
        }
        return false;
      }
    } catch (e) {
      print('目录权限检查异常: $e');
      return false;
    }
  }

  // 获取可用的保存目录
  static Future<String?> getAvailableDirectory() async {
    if (!Platform.isMacOS && !Platform.isWindows && !Platform.isLinux) {
      return null; // 仅桌面平台使用
    }

    try {
      // 1. 检查用户是否跳过了位置提示
      final skipLocationPrompt = await getSkipLocationPrompt();

      // 如果用户跳过了位置提示，检查是否有保存的目录
      if (skipLocationPrompt) {
        final lastDirectory = await getLastDirectory();

        if (lastDirectory.isNotEmpty) {
          print('用户跳过了位置提示，检查上次目录: $lastDirectory');

          // 检查权限是否仍然有效
          final hasAccess = await checkDirectoryAccess(lastDirectory);
          if (hasAccess) {
            print('目录权限有效，使用上次目录');
            return lastDirectory;
          } else {
            print('目录权限失效，清除跳过位置提示设置');
            // 清除设置，让用户重新选择
            await setSkipLocationPrompt(false);
            await setLastDirectory('');
          }
        }
      }

      // 2. 检查是否有上次保存的目录（即使没有跳过位置提示）
      final lastDirectory = await getLastDirectory();
      if (lastDirectory.isNotEmpty) {
        final hasAccess = await checkDirectoryAccess(lastDirectory);
        if (hasAccess) {
          print('使用上次保存的目录: $lastDirectory');
          return lastDirectory;
        } else {
          print('上次保存的目录权限失效: $lastDirectory');
          await setLastDirectory('');
        }
      }

      // 3. 尝试获取默认下载目录
      final downloadsDir = await getDownloadsDirectory();
      if (downloadsDir != null) {
        final downloadsPath = downloadsDir.path;
        final hasAccess = await checkDirectoryAccess(downloadsPath);
        if (hasAccess) {
          print('使用下载目录: $downloadsPath');
          await setLastDirectory(downloadsPath);
          return downloadsPath;
        }
      }

      return null; // 没有可用的目录
    } catch (e) {
      print('获取可用目录失败: $e');
      return null;
    }
  }

  // 请求保存到相册的权限
  static Future<bool> requestSaveToGalleryPermission() async {
    try {
      print('开始请求保存到相册权限...');

      if (Platform.isIOS || Platform.isAndroid) {
        // 使用 gal 包检查相册权限
        final hasAccess = await Gal.hasAccess(toAlbum: true);
        print('Gal 权限检查结果: $hasAccess');

        if (!hasAccess) {
          print('请求 Gal 权限...');
          await Gal.requestAccess(toAlbum: true);

          // 再次检查
          final finalAccess = await Gal.hasAccess(toAlbum: true);
          print('Gal 最终权限结果: $finalAccess');
          return finalAccess;
        }
        return true;
      }
      return true; // 桌面平台不需要特殊权限
    } catch (e) {
      print('Gal 权限请求失败: $e');
      return false;
    }
  }

  // 请求存储权限（用于下载）
  static Future<bool> requestStoragePermission() async {
    try {
      print('开始请求存储权限...');

      if (Platform.isAndroid) {
        // 获取 Android 版本信息
        final deviceInfo = DeviceInfoPlugin();
        final androidInfo = await deviceInfo.androidInfo;
        final sdkInt = androidInfo.version.sdkInt;
        print('Android SDK 版本: $sdkInt');

        if (sdkInt >= 33) {
          // Android 13+ (API 33+) 使用媒体权限
          print('Android 13+ 设备，使用照片权限...');
          final photosStatus = await Permission.photos.status;
          print('照片权限状态: $photosStatus');

          if (photosStatus.isPermanentlyDenied) {
            print('权限被永久拒绝');
            _showErrorToast('请在系统设置中授予照片权限');
            return false;
          }

          if (photosStatus.isDenied || photosStatus.isRestricted) {
            print('请求照片权限...');
            final result = await Permission.photos.request();
            print('照片权限请求结果: $result');
            return result.isGranted;
          }

          print('已有照片权限: $photosStatus');
          return photosStatus.isGranted;
        } else {
          // Android 13 以下使用存储权限
          print('Android 13以下设备，使用存储权限...');
          final storageStatus = await Permission.storage.status;
          print('存储权限状态: $storageStatus');

          if (storageStatus.isPermanentlyDenied) {
            print('存储权限被永久拒绝');
            _showErrorToast('请在系统设置中授予存储权限');
            return false;
          }

          if (storageStatus.isDenied || storageStatus.isRestricted) {
            print('请求存储权限...');
            final result = await Permission.storage.request();
            print('存储权限请求结果: $result');
            return result.isGranted;
          }

          print('已有存储权限: $storageStatus');
          return storageStatus.isGranted;
        }
      } else if (Platform.isIOS) {
        // iOS: 请求相册添加权限
        print('iOS 设备，请求相册权限...');
        final photosStatus = await Permission.photosAddOnly.status;
        print('iOS 相册权限状态: $photosStatus');

        if (photosStatus.isPermanentlyDenied) {
          print('iOS 权限被永久拒绝');
          _showErrorToast('请在系统设置中授予照片权限');
          return false;
        }

        if (photosStatus.isDenied || photosStatus.isRestricted) {
          print('请求iOS相册权限...');
          final result = await Permission.photosAddOnly.request();
          print('iOS相册权限请求结果: $result');
          return result.isGranted;
        }

        print('已有iOS相册权限: $photosStatus');
        return photosStatus.isGranted;
      } else if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
        // 桌面平台：不需要特殊权限
        print('桌面平台，不需要特殊权限');
        return true;
      }

      print('非移动设备，无需权限');
      return true; // 桌面平台不需要特殊权限
    } catch (e) {
      print('权限请求失败: $e');
      _showErrorToast('权限请求失败，请检查系统设置');
      return false;
    }
  }

  // 显示桌面端保存对话框（批量下载时使用同一个目录）
  static Future<String?> _showDesktopSaveDialog(
    BuildContext context,
    String defaultFileName,
    String? videoTitle, // 新增：视频标题参数
    bool isBatchDownload,
  ) async {
    try {
      // 1. 检查用户是否跳过了位置提示
      final skipLocationPrompt = await getSkipLocationPrompt();
      String? availableDir;

      // 只有用户跳过了位置提示才尝试获取可用目录
      if (skipLocationPrompt) {
        availableDir = await getAvailableDirectory();
        print('用户跳过了位置提示，可用目录: $availableDir');
      }

      // 2. 显示统一的保存对话框
      return await _showSaveDialog(
        context,
        defaultFileName,
        videoTitle, // 传递视频标题
        availableDir,
        isBatchDownload,
      );
    } catch (e) {
      print('显示保存对话框失败: $e');
      return null;
    }
  }

  // 统一的保存对话框
  static Future<String?> _showSaveDialog(
    BuildContext context,
    String defaultFileName,
    String? videoTitle,
    String? availableDir,
    bool isBatchDownload,
  ) async {
    try {
      String? selectedDir = availableDir;
      final skipLocationPrompt = await getSkipLocationPrompt();

      // 如果可用目录为空，或者用户没有跳过位置提示，需要用户选择
      if (selectedDir == null || !skipLocationPrompt) {
        print(
          '需要用户选择位置: availableDir=$availableDir, skipLocationPrompt=$skipLocationPrompt',
        );

        selectedDir = await FilePicker.platform.getDirectoryPath(
          dialogTitle: isBatchDownload ? '选择视频保存位置（所有文件将保存到此目录）' : '选择视频保存位置',
        );

        if (selectedDir == null || selectedDir.isEmpty) {
          print('用户取消选择目录');
          return null;
        }

        print('用户选择的目录: $selectedDir');

        // 检查目录权限
        final hasAccess = await checkDirectoryAccess(selectedDir);
        if (!hasAccess) {
          _showErrorToast('没有该目录的写入权限，请选择其他目录');
          return null;
        }
      } else {
        print('使用现有目录: $selectedDir');
      }

      // 使用视频标题作为文件名（清洗非法字符并截断超长标题）
      final fileName = safeFileName(videoTitle ?? defaultFileName);
      final filePath = path.join(selectedDir, '$fileName.mp4');
      print('直接保存到: $filePath');

      // 保存目录设置
      await setLastDirectory(selectedDir);
      return filePath;
    } catch (e) {
      print('显示保存对话框失败: $e');
      return null;
    }
  }

  // 主要下载方法 - 单个视频下载
  static Future<String?> downloadVideo(
    String url,
    String fileName,
    String? videoTitle, // 新增：视频标题参数
    Function(int received, int total)? progressCallback,
    CancelToken? cancelToken,
    BuildContext context, {
    bool isBatchDownload = false,
    String? saveDirectory,
  }) async {
    try {
      print('开始下载流程，平台: ${Platform.operatingSystem}');
      print('视频标题: $videoTitle');

      // 检查权限（仅移动端）
      if (Platform.isAndroid || Platform.isIOS) {
        print('检查移动端下载权限...');
        final hasPermission = await requestStoragePermission();
        print('移动端权限检查结果: $hasPermission');

        if (!hasPermission) {
          _showErrorToast('未授予存储权限，无法下载');
          _showSettingsDialog();
          return null;
        }
      }

      // 获取保存路径
      String? filePath;

      if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
        print('桌面平台，显示保存对话框...');

        // 如果是批量下载且有保存目录，直接使用该目录
        if (isBatchDownload &&
            saveDirectory != null &&
            saveDirectory.isNotEmpty) {
          print('批量下载，使用已有目录: $saveDirectory');

          // 直接使用视频标题作为文件名（清洗非法字符并截断超长标题）
          final name = safeFileName(videoTitle ?? fileName);
          filePath = path.join(saveDirectory, '$name.mp4');
        } else {
          // 单个下载或没有保存目录，显示完整的保存对话框
          filePath = await _showDesktopSaveDialog(
            context,
            fileName,
            videoTitle, // 传递视频标题
            isBatchDownload,
          );
        }

        if (filePath == null) {
          print('用户取消了保存位置选择');
          _showInfoToast('下载已取消');
          return null;
        }
        print('用户选择的路径: $filePath');
      } else if (Platform.isAndroid || Platform.isIOS) {
        // 移动端：使用默认路径，优先使用视频标题作为文件名
        print('移动端平台，使用默认路径...');
        final name = safeFileName(videoTitle ?? fileName);
        filePath = await _getDefaultFilePath(name);
      } else {
        // 其他平台
        print('其他平台，使用下载目录...');
        final name = safeFileName(videoTitle ?? fileName);
        filePath = await _getDesktopFilePath(name);
      }

      if (filePath == null) {
        _showErrorToast('无法确定保存位置');
        return null;
      }

      // 检查文件是否已存在
      final file = File(filePath);
      if (await file.exists()) {
        final fileSize = await file.length();
        print('文件已存在，大小: $fileSize bytes');

        // 批量下载时不询问覆盖，自动使用新文件名
        if (isBatchDownload) {
          final baseName = safeFileName(videoTitle ?? fileName);
          final newFileName = await _generateUniqueFileName(
            path.dirname(filePath),
            baseName,
          );
          filePath = path.join(path.dirname(filePath), '$newFileName.mp4');
          print('文件已存在，使用新文件名: $filePath');
        } else {
          // 询问是否覆盖
          final shouldOverwrite = await _showOverwriteDialog(
            context,
            filePath,
            fileSize,
          );
          if (!shouldOverwrite) {
            _showInfoToast('下载已取消');
            return null;
          }
        }
      }

      // 显示开始下载提示
      if (!isBatchDownload) {
        final displayName = videoTitle ?? fileName;
        _showInfoToast('开始下载视频: $displayName');
      }

      // 下载文件
      _currentCancelToken = cancelToken ?? CancelToken();

      print('开始下载视频，URL: $url');
      print('保存到: $filePath');

      final sessionDir = await _createDownloadSessionDirectory(filePath);
      final probe = await _probeRemoteFileInfo(url, _currentCancelToken);

      String tempDownloadedPath;
      int? expectedTotalSize = probe.totalSize;

      if (probe.supportsRange &&
          expectedTotalSize != null &&
          expectedTotalSize > 0) {
        try {
          tempDownloadedPath = await _downloadByChunksToTempFile(
            url: url,
            sessionDir: sessionDir,
            totalSize: expectedTotalSize,
            cancelToken: _currentCancelToken,
            progressCallback: progressCallback,
          );
        } catch (e) {
          print('分块下载失败，回退到单线程下载: $e');
          final singleTempPath = path.join(sessionDir.path, 'single.tmp');
          int lastTotal = 0;
          await _downloadSingleToPath(
            url,
            singleTempPath,
            _currentCancelToken,
            (received, total) {
              lastTotal = total;
              if (progressCallback != null) {
                progressCallback(received, total);
              }
            },
          );
          tempDownloadedPath = singleTempPath;
          if (expectedTotalSize <= 0 && lastTotal > 0) {
            expectedTotalSize = lastTotal;
          }
        }
      } else {
        final singleTempPath = path.join(sessionDir.path, 'single.tmp');
        int lastTotal = 0;
        await _downloadSingleToPath(url, singleTempPath, _currentCancelToken, (
          received,
          total,
        ) {
          lastTotal = total;
          if (progressCallback != null) {
            progressCallback(received, total);
          }
        });
        tempDownloadedPath = singleTempPath;
        if ((expectedTotalSize == null || expectedTotalSize <= 0) &&
            lastTotal > 0) {
          expectedTotalSize = lastTotal;
        }
      }

      final tempFile = File(tempDownloadedPath);
      if (!await tempFile.exists()) {
        _showErrorToast('下载完成但临时文件未找到');
        return null;
      }

      if (expectedTotalSize != null && expectedTotalSize > 0) {
        final actualSize = await tempFile.length();
        if (actualSize != expectedTotalSize) {
          _showErrorToast('下载校验失败: $actualSize != $expectedTotalSize');
          return null;
        }
      }

      await _replaceFile(tempDownloadedPath, filePath);
      try {
        await sessionDir.delete(recursive: true);
      } catch (_) {}

      // 检查文件是否下载成功
      final downloadedFile = File(filePath);
      if (await downloadedFile.exists()) {
        final fileSize = await downloadedFile.length();
        print('下载完成，文件大小: $fileSize bytes');

        // 批量下载时不单独显示完成通知
        if (!isBatchDownload) {
          final displayName = videoTitle ?? fileName;
          _showSuccessToast(
            '"$displayName" 下载完成 (${formatFileSize(fileSize)})',
          );

          // 下载完成后询问用户操作（仅桌面端）
          if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
            await _showDownloadCompleteDialog(filePath, fileSize, context);
          }
        }

        return filePath;
      } else {
        _showErrorToast('下载完成但文件未找到');
        return null;
      }
    } on DioException catch (e) {
      print('Dio下载异常: ${e.type}, ${e.message}');
      if (e.type == DioExceptionType.cancel) {
        _showInfoToast('下载已取消');
      } else if (e.type == DioExceptionType.receiveTimeout) {
        _showErrorToast('下载超时，请检查网络连接');
      } else {
        _showErrorToast('下载失败: ${e.message}');
      }
      return null;
    } catch (e) {
      print('下载失败异常: $e');
      _showErrorToast('下载失败: ${e.toString()}');
      return null;
    } finally {
      _currentCancelToken = null;
    }
  }

  // 批量顺序下载方法
  static Future<List<String?>> downloadMultipleVideos(
    List<String> urls,
    List<String> videoTitles, // 新增：视频标题列表
    String baseFileName,
    Function(int index, int received, int total)? progressCallback,
    CancelToken? cancelToken,
    BuildContext context,
  ) async {
    final results = <String?>[];
    String? saveDirectory;

    try {
      // 先获取保存目录（只问一次）
      if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
        final filePath = await _showDesktopSaveDialog(
          context,
          baseFileName,
          null, // 批量下载时不传递单个标题
          true, // 标记为批量下载
        );

        if (filePath == null) {
          print('用户取消了批量下载位置选择');
          _showInfoToast('批量下载已取消');
          return results;
        }

        saveDirectory = path.dirname(filePath);
        print('批量下载保存目录: $saveDirectory');
      }

      for (var i = 0; i < urls.length; i++) {
        if (cancelToken?.isCancelled == true) {
          break;
        }

        final url = urls[i];
        final videoTitle = i < videoTitles.length ? videoTitles[i] : null;
        final fileName = videoTitle ?? '${baseFileName}_${i + 1}';

        print('开始下载第 ${i + 1}/${urls.length} 个视频: $fileName');

        final filePath = await downloadVideo(
          url,
          fileName,
          videoTitle, // 传递视频标题
          (received, total) {
            if (progressCallback != null) {
              progressCallback(i, received, total);
            }
          },
          cancelToken,
          context,
          isBatchDownload: true,
          saveDirectory: saveDirectory,
        );

        results.add(filePath);

        // 等待1秒再下载下一个，避免请求过于频繁
        if (i < urls.length - 1) {
          await Future.delayed(Duration(seconds: 1));
        }
      }

      return results;
    } catch (e) {
      print('批量下载异常: $e');
      return results;
    }
  }

  // 生成唯一文件名
  static Future<String> _generateUniqueFileName(
    String directory,
    String baseName,
  ) async {
    var fileName = baseName;
    var counter = 1;

    while (true) {
      final filePath = path.join(directory, '$fileName.mp4');
      final file = File(filePath);

      if (!await file.exists()) {
        return fileName;
      }

      fileName = '${baseName}_$counter';
      counter++;
    }
  }

  // 获取默认文件路径（移动端）
  static Future<String?> _getDefaultFilePath(String fileName) async {
    try {
      final baseDir = await _getDownloadCacheDirectory();
      final downloadFolder = Directory(path.join(baseDir.path, 'downloads'));
      if (!await downloadFolder.exists()) {
        await downloadFolder.create(recursive: true);
      }
      return path.join(downloadFolder.path, '$fileName.mp4');
    } catch (e) {
      print('获取默认路径失败: $e');
      return null;
    }
  }

  // 获取桌面端文件路径
  static Future<String?> _getDesktopFilePath(String fileName) async {
    try {
      final downloadsDir = await getDownloadsDirectory();
      final downloadDir =
          downloadsDir ?? await getApplicationDocumentsDirectory();
      return path.join(downloadDir.path, '$fileName.mp4');
    } catch (e) {
      print('获取桌面端路径失败: $e');
      return null;
    }
  }

  // 显示覆盖确认对话框
  static Future<bool> _showOverwriteDialog(
    BuildContext context,
    String filePath,
    int fileSize,
  ) async {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    // 浅色模式颜色定义
    const Color lightEnabledColor = Color(0xFF409EFF);
    const Color lightEnabledBgColor = Color(0xFFECF5FF);
    const Color lightEnabledBorderColor = Color(0xFFB3D8FF);

    // 深色模式颜色定义
    const Color darkClearBgColor = Color(0xFF18232B); // 清空按钮背景色
    const Color darkClearBorderColor = Color(0xFF409EFF); // 清空按钮边框色
    const Color darkClearTextColor = Color(0xFF409EFF); // 清空按钮文本色

    return await showDialog<bool>(
          context: context,
          barrierDismissible: false,
          builder: (context) {
            return AlertDialog(
              title: const Text('文件已存在'),
              content: Text(
                '文件 ${path.basename(filePath)} 已存在，大小为 ${formatFileSize(fileSize)}。\n\n是否要覆盖？',
              ),
              actions: [
                // 取消按钮 - 使用解析按钮的样式
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
                  child: TextButton(
                    onPressed: () {
                      Navigator.of(context).pop(false);
                    },
                    style: TextButton.styleFrom(
                      padding: const EdgeInsets.symmetric(
                        vertical: 12,
                        horizontal: 16,
                      ),
                      backgroundColor:
                          isDarkMode ? darkClearBgColor : lightEnabledBgColor,
                      foregroundColor:
                          isDarkMode ? darkClearTextColor : lightEnabledColor,
                      side: BorderSide(
                        color:
                            isDarkMode
                                ? darkClearBorderColor
                                : lightEnabledBorderColor,
                        width: 1,
                      ),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(4),
                      ),
                    ),
                    child: const Text('取消'),
                  ),
                ),
                // 覆盖按钮 - 使用解析按钮的样式
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
                      Navigator.of(context).pop(true);
                    },
                    style: ElevatedButton.styleFrom(
                      padding: const EdgeInsets.symmetric(
                        vertical: 12,
                        horizontal: 16,
                      ),
                      backgroundColor: primaryColor,
                      foregroundColor:
                          isDarkMode ? Colors.white : onPrimaryColor,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(4),
                      ),
                      elevation: 0,
                    ),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          Icons.file_download,
                          size: 14,
                          color: isDarkMode ? Colors.white : onPrimaryColor,
                        ),
                        const SizedBox(width: 2),
                        const Text('覆盖'),
                      ],
                    ),
                  ),
                ),
              ],
            );
          },
        ) ??
        false;
  }

  // 显示下载完成对话框（仅桌面端）
  static Future<void> _showDownloadCompleteDialog(
    String filePath,
    int fileSize,
    BuildContext context,
  ) async {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    // 浅色模式颜色定义
    const Color lightEnabledColor = Color(0xFF409EFF);
    const Color lightEnabledBgColor = Color(0xFFECF5FF);
    const Color lightEnabledBorderColor = Color(0xFFB3D8FF);

    // 深色模式颜色定义
    const Color darkClearBgColor = Color(0xFF18232B); // 清空按钮背景色
    const Color darkClearBorderColor = Color(0xFF409EFF); // 清空按钮边框色
    const Color darkClearTextColor = Color(0xFF409EFF); // 清空按钮文本色

    final fileName = path.basename(filePath);
    final formattedSize = formatFileSize(fileSize);

    // 保存对话框上下文，用于后续手动关闭
    BuildContext? dialogContext;

    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (context) {
        dialogContext = context;
        return AlertDialog(
          title: const Text('下载完成'),
          content: Text('视频文件 "$fileName" 已下载完成，大小为 $formattedSize。\n\n您希望：'),
          actions: [
            // 打开文件按钮
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
              child: TextButton(
                onPressed: () async {
                  // 打开文件但不关闭对话框
                  try {
                    await OpenFile.open(filePath);
                  } catch (e) {
                    print('打开文件失败: $e');
                    _showErrorToast('无法打开文件: $e');
                  }
                },
                style: TextButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    vertical: 12,
                    horizontal: 16,
                  ),
                  backgroundColor:
                      isDarkMode ? darkClearBgColor : lightEnabledBgColor,
                  foregroundColor:
                      isDarkMode ? darkClearTextColor : lightEnabledColor,
                  side: BorderSide(
                    color:
                        isDarkMode
                            ? darkClearBorderColor
                            : lightEnabledBorderColor,
                    width: 1,
                  ),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(4),
                  ),
                ),
                child: const Text('打开文件'),
              ),
            ),
            if (Platform.isMacOS)
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
                child: TextButton(
                  onPressed: () async {
                    // 在Finder中显示但不关闭对话框
                    try {
                      await Process.run('open', ['-R', filePath]);
                    } catch (e) {
                      print('在Finder中显示失败: $e');
                      await OpenFile.open(path.dirname(filePath));
                    }
                  },
                  style: TextButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      vertical: 12,
                      horizontal: 16,
                    ),
                    backgroundColor:
                        isDarkMode ? darkClearBgColor : lightEnabledBgColor,
                    foregroundColor:
                        isDarkMode ? darkClearTextColor : lightEnabledColor,
                    side: BorderSide(
                      color:
                          isDarkMode
                              ? darkClearBorderColor
                              : lightEnabledBorderColor,
                      width: 1,
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(4),
                    ),
                  ),
                  child: const Text('在Finder中显示'),
                ),
              ),
            if (Platform.isWindows)
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
                child: TextButton(
                  onPressed: () async {
                    // 在资源管理器中显示但不关闭对话框
                    try {
                      await Process.run('explorer', ['/select,', filePath]);
                    } catch (e) {
                      print('在资源管理器中显示失败: $e');
                      await OpenFile.open(path.dirname(filePath));
                    }
                  },
                  style: TextButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      vertical: 12,
                      horizontal: 16,
                    ),
                    backgroundColor:
                        isDarkMode ? darkClearBgColor : lightEnabledBgColor,
                    foregroundColor:
                        isDarkMode ? darkClearTextColor : lightEnabledColor,
                    side: BorderSide(
                      color:
                          isDarkMode
                              ? darkClearBorderColor
                              : lightEnabledBorderColor,
                      width: 1,
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(4),
                    ),
                  ),
                  child: const Text('在文件夹中显示'),
                ),
              ),
            // 关闭按钮 - 使用解析按钮的样式
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
                  // 只有点击关闭才关闭对话框
                  if (dialogContext != null &&
                      Navigator.canPop(dialogContext!)) {
                    Navigator.of(dialogContext!).pop();
                  }
                },
                style: ElevatedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    vertical: 12,
                    horizontal: 16,
                  ),
                  backgroundColor: primaryColor,
                  foregroundColor: isDarkMode ? Colors.white : onPrimaryColor,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(4),
                  ),
                  elevation: 0,
                ),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Icon(
                      Icons.close,
                      size: 14,
                      color: isDarkMode ? Colors.white : onPrimaryColor,
                    ),
                    const SizedBox(width: 2),
                    const Text('关闭'),
                  ],
                ),
              ),
            ),
          ],
        );
      },
    );
  }

  // 显示去设置对话框
  static void _showSettingsDialog() {
    BotToast.showAttachedWidget(
      attachedBuilder:
          (_) => Card(
            margin: const EdgeInsets.all(10),
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    '权限被拒绝',
                    style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 8),
                  const Text('请在系统设置中授予存储权限以继续下载'),
                  const SizedBox(height: 16),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      TextButton(
                        onPressed: () => BotToast.cleanAll(),
                        child: const Text('取消'),
                      ),
                      const SizedBox(width: 8),
                      ElevatedButton(
                        onPressed: () {
                          BotToast.cleanAll();
                          openAppSettings();
                        },
                        child: const Text('去设置'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
      duration: const Duration(seconds: 10),
      target: const Offset(0, 0),
      preferDirection: PreferDirection.topRight,
    );
  }

  // 保存视频到相册（仅移动端）
  static Future<void> saveVideoToGallery(String filePath) async {
    try {
      print('开始保存视频到相册: $filePath');
      final file = File(filePath);
      if (!await file.exists()) {
        _showErrorToast('文件不存在');
        return;
      }

      if (Platform.isIOS || Platform.isAndroid) {
        // 先检查相册保存权限
        _showInfoToast('正在检查相册权限...');

        final hasGalleryPermission = await requestSaveToGalleryPermission();
        if (!hasGalleryPermission) {
          _showErrorToast('未授予保存到相册的权限');
          return;
        }

        // 使用 gal 保存到相册
        _showInfoToast('正在保存到相册...');
        print('使用 Gal 保存视频到相册...');

        try {
          await Gal.putVideo(filePath, album: '抖音视频');

          print('Gal 保存成功');
          _showSuccessToast('视频已成功保存到相册');
          try {
            await file.delete();
          } catch (_) {}
        } catch (e) {
          print('Gal 保存失败: $e');
          _showErrorToast('保存到相册失败，请重试');
        }
      } else {
        // 桌面端：直接打开文件
        await OpenFile.open(filePath);
      }
    } catch (e) {
      print('保存失败: $e');
      _showErrorToast('保存失败: ${e.toString()}');
    }
  }

  // 批量保存视频到相册（仅移动端）
  static Future<void> saveMultipleVideosToGallery(
    List<String> filePaths,
  ) async {
    try {
      print('开始批量保存 ${filePaths.length} 个视频到相册');

      if (Platform.isIOS || Platform.isAndroid) {
        // 先检查相册保存权限
        _showInfoToast('正在检查相册权限...');

        final hasGalleryPermission = await requestSaveToGalleryPermission();
        if (!hasGalleryPermission) {
          _showErrorToast('未授予保存到相册的权限');
          return;
        }

        int successCount = 0;
        int failCount = 0;

        for (var i = 0; i < filePaths.length; i++) {
          final filePath = filePaths[i];
          final file = File(filePath);

          if (!await file.exists()) {
            print('文件不存在: $filePath');
            failCount++;
            continue;
          }

          try {
            await Gal.putVideo(filePath, album: '抖音视频');
            print('第 ${i + 1} 个视频保存成功: $filePath');
            successCount++;
            try {
              await file.delete();
            } catch (_) {}
          } catch (e) {
            print('第 ${i + 1} 个视频保存失败: $e');
            failCount++;
          }

          // 延迟一下，避免请求过于频繁
          if (i < filePaths.length - 1) {
            await Future.delayed(Duration(milliseconds: 500));
          }
        }

        if (successCount > 0) {
          if (failCount == 0) {
            _showSuccessToast('$successCount 个视频已全部保存到相册');
          } else {
            _showSuccessToast('$successCount 个视频保存成功，$failCount 个失败');
          }
        } else {
          _showErrorToast('所有视频保存到相册失败');
        }
      } else {
        // 桌面端：只显示完成通知
        _showSuccessToast('${filePaths.length} 个视频下载完成');
      }
    } catch (e) {
      print('批量保存失败: $e');
      _showErrorToast('批量保存失败: ${e.toString()}');
    }
  }

  // 公共访问方法
  static Future<bool> getSkipFilenameCustomization() async {
    return await _getSetting(_skipFilenameCustomKey, false) as bool;
  }

  static Future<bool> getSkipLocationPrompt() async {
    return await _getSetting(_skipLocationPromptKey, false) as bool;
  }

  static Future<String> getLastDirectory() async {
    return await _getSetting(_lastDirectoryKey, '') as String;
  }

  static Future<bool> getRememberLocation() async {
    return await _getSetting(_rememberLocationKey, false) as bool;
  }

  // 公共设置方法
  static Future<void> setSkipFilenameCustomization(bool value) async {
    await _saveSetting(_skipFilenameCustomKey, value);
  }

  static Future<void> setSkipLocationPrompt(bool value) async {
    await _saveSetting(_skipLocationPromptKey, value);
  }

  static Future<void> setLastDirectory(String value) async {
    await _saveSetting(_lastDirectoryKey, value);
  }

  static Future<void> setRememberLocation(bool value) async {
    await _saveSetting(_rememberLocationKey, value);
  }

  // 获取当前保存位置
  static Future<String?> getCurrentSaveLocation() async {
    final location = await getLastDirectory();
    return location.isNotEmpty ? location : null;
  }

  // 设置保存位置
  static Future<void> setSaveLocation(String path) async {
    await setLastDirectory(path);
  }

  // 重置保存位置设置
  static Future<void> resetSaveLocationSettings() async {
    await _saveSetting(_skipLocationPromptKey, false);
    await _saveSetting(_skipFilenameCustomKey, false);
    await _saveSetting(_lastDirectoryKey, '');
    await _saveSetting(_rememberLocationKey, false);
    if (Platform.isMacOS) {
      await _saveSetting(_hasMacOSPermissionKey, false);
    }
    print('保存位置设置已重置');
  }

  static String generateFileName() {
    final now = DateTime.now();
    return 'douyin_${now.year}${now.month.toString().padLeft(2, '0')}${now.day.toString().padLeft(2, '0')}_${now.hour}${now.minute}${now.second}';
  }

  static void cancelCurrentDownload() {
    if (_currentCancelToken != null && !_currentCancelToken!.isCancelled) {
      _currentCancelToken!.cancel();
    }
  }

  static bool isDownloading() {
    return _currentCancelToken != null && !_currentCancelToken!.isCancelled;
  }

  // Toast 辅助方法
  static void _showSuccessToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 3),
      contentColor: Colors.green,
      textStyle: const TextStyle(color: Colors.white, fontSize: 14),
    );
  }

  static void _showErrorToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 4),
      contentColor: Colors.red,
      textStyle: const TextStyle(color: Colors.white, fontSize: 14),
    );
  }

  static void _showInfoToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
  }
}

class _RemoteFileInfo {
  final bool supportsRange;
  final int? totalSize;

  const _RemoteFileInfo({required this.supportsRange, required this.totalSize});
}
