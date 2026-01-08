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

  // 存储设置的键名
  static const String _rememberLocationKey = 'remember_download_location';
  static const String _skipFilenameCustomKey = 'skip_filename_customization';
  static const String _lastDirectoryKey = 'last_download_directory';
  static const String _hasMacOSPermissionKey = 'macos_has_directory_permission';

  // 添加 formatFileSize 为公共静态方法
  static String formatFileSize(int bytes) {
    if (bytes <= 0) return "0 B";
    const suffixes = ["B", "KB", "MB", "GB", "TB"];
    final i = (log(bytes) / log(1024)).floor();
    return '${(bytes / pow(1024, i)).toStringAsFixed(1)} ${suffixes[i]}';
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

  // 检查macOS是否有目录访问权限（带缓存）
  static Future<bool> checkMacOSDirectoryAccess(String directoryPath) async {
    if (!Platform.isMacOS) return true;

    try {
      // 检查目录是否存在
      final dir = Directory(directoryPath);
      if (!await dir.exists()) {
        print('macOS目录不存在: $directoryPath');
        return false;
      }

      // 尝试在目录中创建一个临时文件来测试权限
      final tempFilePath =
          '$directoryPath/.douyin_permission_test_${DateTime.now().millisecondsSinceEpoch}.tmp';
      final tempFile = File(tempFilePath);

      try {
        // 尝试写入
        await tempFile.writeAsString('permission_test', flush: true);

        // 尝试读取
        await tempFile.readAsString();

        // 清理临时文件
        if (await tempFile.exists()) {
          await tempFile.delete();
        }

        print('macOS目录权限检查通过: $directoryPath');
        return true;
      } catch (e) {
        print('macOS目录权限检查失败: $e');
        // 清理可能创建的临时文件
        if (await tempFile.exists()) {
          try {
            await tempFile.delete();
          } catch (_) {}
        }
        return false;
      }
    } catch (e) {
      print('macOS目录权限检查异常: $e');
      return false;
    }
  }

  // 获取macOS可用的保存目录
  static Future<String?> getAvailableMacOSDirectory() async {
    if (!Platform.isMacOS) return null;

    try {
      // 1. 先检查是否保存了上次的目录
      final lastDirectory = await _getSetting(_lastDirectoryKey, '') as String;
      final hasPermissionFlag =
          await _getSetting(_hasMacOSPermissionKey, false) as bool;

      if (lastDirectory.isNotEmpty && hasPermissionFlag) {
        // 验证目录权限是否仍然有效
        final hasAccess = await checkMacOSDirectoryAccess(lastDirectory);
        if (hasAccess) {
          print('macOS使用上次保存的目录（权限有效）: $lastDirectory');
          return lastDirectory;
        } else {
          print('macOS上次保存的目录权限已失效: $lastDirectory');
          // 清除权限标志
          await _saveSetting(_hasMacOSPermissionKey, false);
        }
      }

      // 2. 尝试获取默认下载目录
      final downloadsDir = await getDownloadsDirectory();
      if (downloadsDir != null) {
        final downloadsPath = downloadsDir.path;
        final hasAccess = await checkMacOSDirectoryAccess(downloadsPath);
        if (hasAccess) {
          print('macOS使用下载目录: $downloadsPath');
          await _saveSetting(_lastDirectoryKey, downloadsPath);
          await _saveSetting(_hasMacOSPermissionKey, true);
          return downloadsPath;
        }
      }

      // 3. 尝试获取文档目录
      final documentsDir = await getApplicationDocumentsDirectory();
      final documentsPath = documentsDir.path;
      final hasAccess = await checkMacOSDirectoryAccess(documentsPath);
      if (hasAccess) {
        print('macOS使用文档目录: $documentsPath');
        await _saveSetting(_lastDirectoryKey, documentsPath);
        await _saveSetting(_hasMacOSPermissionKey, true);
        return documentsPath;
      }

      return null;
    } catch (e) {
      print('获取macOS可用目录失败: $e');
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

  // 显示桌面端保存对话框（macOS和Windows）
  static Future<String?> _showDesktopSaveDialog(
    BuildContext context,
    String defaultFileName,
  ) async {
    try {
      final platform = Platform.operatingSystem;
      print('显示${platform}保存对话框...');

      // macOS：尝试获取可用的目录
      if (Platform.isMacOS) {
        final availableDir = await getAvailableMacOSDirectory();
        if (availableDir != null) {
          print('macOS找到可用目录: $availableDir');

          // 检查是否跳过文件名自定义
          final skipFilenameCustom =
              await _getSetting(_skipFilenameCustomKey, false) as bool;
          if (skipFilenameCustom) {
            // 跳过文件名自定义，直接使用默认文件名
            final filePath = path.join(availableDir, '$defaultFileName.mp4');
            print('macOS跳过文件名自定义，直接保存到: $filePath');
            return filePath;
          } else {
            // 显示文件名输入对话框
            return await _showFilenameInputDialog(
              context,
              availableDir,
              defaultFileName,
            );
          }
        } else {
          print('macOS没有可用的目录，需要用户选择');
          return await _promptUserForSaveLocation(context, defaultFileName);
        }
      }

      // Windows：检查用户设置
      if (Platform.isWindows) {
        final rememberLocation =
            await _getSetting(_rememberLocationKey, false) as bool;
        final skipFilenameCustom =
            await _getSetting(_skipFilenameCustomKey, false) as bool;
        final lastDirectory =
            await _getSetting(_lastDirectoryKey, '') as String;

        // 如果记住位置且有上次保存的目录，并且目录存在，直接使用
        if (rememberLocation &&
            lastDirectory.isNotEmpty &&
            Directory(lastDirectory).existsSync()) {
          print('Windows：使用上次保存的目录: $lastDirectory');

          if (skipFilenameCustom) {
            // 跳过文件名自定义，直接使用默认文件名
            final filePath = path.join(lastDirectory, '$defaultFileName.mp4');
            print('跳过文件名自定义，直接保存到: $filePath');
            return filePath;
          } else {
            // 需要自定义文件名
            return await _showFilenameInputDialog(
              context,
              lastDirectory,
              defaultFileName,
            );
          }
        }
      }

      // 显示保存对话框让用户选择位置
      return await _promptUserForSaveLocation(context, defaultFileName);
    } catch (e) {
      print('显示保存对话框失败: $e');
      return null;
    }
  }

  // 提示用户选择保存位置（核心方法）
  static Future<String?> _promptUserForSaveLocation(
    BuildContext context,
    String defaultFileName,
  ) async {
    try {
      _showInfoToast('请选择保存位置...');

      // 1. 先让用户选择目录
      String? selectedDir = await FilePicker.platform.getDirectoryPath(
        dialogTitle: '选择视频保存位置',
      );

      if (selectedDir == null || selectedDir.isEmpty) {
        print('用户取消选择目录');
        return null;
      }

      print('用户选择的目录: $selectedDir');

      // 检查目录权限
      final hasAccess = await checkMacOSDirectoryAccess(selectedDir);
      if (!hasAccess) {
        _showErrorToast('没有该目录的写入权限，请选择其他目录');
        return null;
      }

      // 保存选择的目录和权限状态
      await _saveSetting(_lastDirectoryKey, selectedDir);
      if (Platform.isMacOS) {
        await _saveSetting(_hasMacOSPermissionKey, true);
      }

      // 检查是否跳过文件名自定义
      final skipFilenameCustom =
          await _getSetting(_skipFilenameCustomKey, false) as bool;
      if (skipFilenameCustom) {
        // 跳过文件名自定义，直接使用默认文件名
        final filePath = path.join(selectedDir, '$defaultFileName.mp4');
        print('跳过文件名自定义，直接保存到: $filePath');
        return filePath;
      }

      // 显示对话框让用户输入文件名和选择选项
      return await _showFilenameInputDialog(
        context,
        selectedDir,
        defaultFileName,
      );
    } catch (e) {
      print('提示用户选择保存位置失败: $e');
      return null;
    }
  }

  // 显示文件名输入对话框（带复选框选项）
  static Future<String?> _showFilenameInputDialog(
    BuildContext context,
    String selectedDir,
    String defaultFileName,
  ) async {
    try {
      // 读取设置
      bool rememberLocation = false;
      bool skipFilenameCustom =
          await _getSetting(_skipFilenameCustomKey, false) as bool;

      // 只有Windows显示"记住位置"选项
      if (Platform.isWindows) {
        rememberLocation =
            await _getSetting(_rememberLocationKey, false) as bool;
      }

      final TextEditingController controller = TextEditingController(
        text: defaultFileName,
      );

      final result = await showDialog<Map<String, dynamic>>(
        context: context,
        barrierDismissible: false,
        builder: (context) {
          return StatefulBuilder(
            builder: (context, setState) {
              return AlertDialog(
                title: Text('保存视频文件'),
                content: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('保存到: $selectedDir'),
                      SizedBox(height: 16),
                      TextField(
                        controller: controller,
                        decoration: InputDecoration(
                          labelText: '文件名',
                          border: OutlineInputBorder(),
                          suffixText: '.mp4',
                        ),
                        autofocus: true,
                      ),
                      SizedBox(height: 16),
                      // 只有Windows显示"记住此保存位置"
                      if (Platform.isWindows)
                        CheckboxListTile(
                          title: Text('记住此保存位置'),
                          value: rememberLocation,
                          onChanged: (value) {
                            setState(() {
                              rememberLocation = value ?? false;
                            });
                          },
                          dense: true,
                          contentPadding: EdgeInsets.zero,
                        ),
                      // Windows和macOS都显示"不再提示文件名自定义"
                      CheckboxListTile(
                        title: Text('不再提示文件名自定义'),
                        subtitle: Text('下次直接保存，不再弹出此窗口'),
                        value: skipFilenameCustom,
                        onChanged: (value) {
                          setState(() {
                            skipFilenameCustom = value ?? false;
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
                    onPressed: () => Navigator.of(context).pop(null),
                    child: Text('取消'),
                  ),
                  ElevatedButton(
                    onPressed: () {
                      String fileName = controller.text.trim();
                      if (fileName.isEmpty) {
                        fileName = defaultFileName;
                      }
                      Navigator.of(context).pop({
                        'fileName': fileName,
                        'rememberLocation': rememberLocation,
                        'skipFilenameCustom': skipFilenameCustom,
                      });
                    },
                    child: Text('保存'),
                  ),
                ],
              );
            },
          );
        },
      );

      if (result == null) {
        return null;
      }

      // 保存用户设置
      if (Platform.isWindows) {
        await _saveSetting(_rememberLocationKey, result['rememberLocation']);
      }

      // Windows和macOS都保存"跳过文件名自定义"设置
      await _saveSetting(_skipFilenameCustomKey, result['skipFilenameCustom']);

      // 构建完整路径
      String fileName = result['fileName'];
      if (!fileName.toLowerCase().endsWith('.mp4')) {
        fileName = '$fileName.mp4';
      }

      final fullPath = path.join(selectedDir, fileName);
      print('最终保存路径: $fullPath');

      return fullPath;
    } catch (e) {
      print('显示文件名输入对话框失败: $e');
      return null;
    }
  }

  // 主要下载方法
  static Future<String?> downloadVideo(
    String url,
    String fileName,
    Function(int received, int total)? progressCallback,
    CancelToken? cancelToken,
    BuildContext context, // 这里改为非可选参数
  ) async {
    try {
      print('开始下载流程，平台: ${Platform.operatingSystem}');

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

      if (Platform.isMacOS || Platform.isWindows) {
        print('桌面平台，显示保存对话框...');
        // 显示保存对话框
        filePath = await _showDesktopSaveDialog(context, fileName);
        if (filePath == null) {
          print('用户取消了保存位置选择');
          _showInfoToast('下载已取消');
          return null;
        }
        print('用户选择的路径: $filePath');
      } else if (Platform.isAndroid || Platform.isIOS) {
        // 移动端：使用默认路径，然后直接保存到相册
        print('移动端平台，使用默认路径...');
        filePath = await _getDefaultFilePath(fileName);
      } else {
        // Linux：使用下载目录
        print('Linux平台，使用下载目录...');
        filePath = await _getDesktopFilePath(fileName);
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

      // 显示开始下载提示
      _showInfoToast('开始下载视频...');

      // 下载文件
      _currentCancelToken = cancelToken ?? CancelToken();

      print('开始下载视频，URL: $url');
      print('保存到: $filePath');

      // 确保目录存在
      final dir = Directory(path.dirname(filePath));
      if (!await dir.exists()) {
        print('创建目录: ${dir.path}');
        await dir.create(recursive: true);
      }

      await _dio.download(
        url,
        filePath,
        cancelToken: _currentCancelToken,
        onReceiveProgress: (received, total) {
          if (progressCallback != null) {
            progressCallback(received, total);
          }
        },
        options: Options(
          headers: {
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
          },
          receiveTimeout: const Duration(minutes: 10),
          sendTimeout: const Duration(minutes: 10),
        ),
      );

      // 检查文件是否下载成功
      final downloadedFile = File(filePath);
      if (await downloadedFile.exists()) {
        final fileSize = await downloadedFile.length();
        print('下载完成，文件大小: $fileSize bytes');
        _showSuccessToast('下载完成 (${formatFileSize(fileSize)})');

        // 下载完成后询问用户操作（仅桌面端）
        if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
          await _showDownloadCompleteDialog(filePath, fileSize, context);
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

  // 获取默认文件路径（移动端）
  static Future<String?> _getDefaultFilePath(String fileName) async {
    try {
      Directory downloadDir;

      if (Platform.isAndroid) {
        print('Android设备，获取外部存储目录...');
        final externalDir = await getExternalStorageDirectory();
        downloadDir = externalDir ?? await getApplicationDocumentsDirectory();

        // 创建下载文件夹
        final downloadFolder = Directory('${downloadDir.path}/DouyinVideos');
        if (!await downloadFolder.exists()) {
          await downloadFolder.create(recursive: true);
        }
        return '${downloadFolder.path}/$fileName.mp4';
      } else if (Platform.isIOS) {
        print('iOS设备，获取应用文档目录...');
        downloadDir = await getApplicationDocumentsDirectory();
        return '${downloadDir.path}/$fileName.mp4';
      }

      return null;
    } catch (e) {
      print('获取默认路径失败: $e');
      return null;
    }
  }

  // 获取桌面端文件路径（Linux）
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
                TextButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: const Text('取消'),
                ),
                ElevatedButton(
                  onPressed: () {
                    Navigator.of(context).pop(true);
                  },
                  child: const Text('覆盖'),
                ),
              ],
            );
          },
        ) ??
        false;
  }

  // 显示下载完成对话框（仅桌面端）- 修改为不自动关闭
  static Future<void> _showDownloadCompleteDialog(
    String filePath,
    int fileSize,
    BuildContext context,
  ) async {
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
            TextButton(
              onPressed: () async {
                // 打开文件但不关闭对话框
                try {
                  await OpenFile.open(filePath);
                } catch (e) {
                  print('打开文件失败: $e');
                  _showErrorToast('无法打开文件: $e');
                }
              },
              child: const Text('打开文件'),
            ),
            if (Platform.isMacOS)
              TextButton(
                onPressed: () async {
                  // 在Finder中显示但不关闭对话框
                  try {
                    await Process.run('open', ['-R', filePath]);
                  } catch (e) {
                    print('在Finder中显示失败: $e');
                    await OpenFile.open(path.dirname(filePath));
                  }
                },
                child: const Text('在Finder中显示'),
              ),
            if (Platform.isWindows)
              TextButton(
                onPressed: () async {
                  // 在资源管理器中显示但不关闭对话框
                  try {
                    await Process.run('explorer', ['/select,', filePath]);
                  } catch (e) {
                    print('在资源管理器中显示失败: $e');
                    await OpenFile.open(path.dirname(filePath));
                  }
                },
                child: const Text('在文件夹中显示'),
              ),
            TextButton(
              onPressed: () {
                // 只有点击关闭才关闭对话框
                if (dialogContext != null && Navigator.canPop(dialogContext!)) {
                  Navigator.of(dialogContext!).pop();
                }
              },
              child: const Text('关闭'),
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
            margin: EdgeInsets.all(10),
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    '权限被拒绝',
                    style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
                  ),
                  SizedBox(height: 8),
                  Text('请在系统设置中授予存储权限以继续下载'),
                  SizedBox(height: 16),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      TextButton(
                        onPressed: () => BotToast.cleanAll(),
                        child: Text('取消'),
                      ),
                      SizedBox(width: 8),
                      ElevatedButton(
                        onPressed: () {
                          BotToast.cleanAll();
                          openAppSettings();
                        },
                        child: Text('去设置'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
      duration: Duration(seconds: 10),
      target: Offset(0, 0),
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
      duration: Duration(seconds: 3),
      contentColor: Colors.green,
      textStyle: TextStyle(color: Colors.white, fontSize: 14),
    );
  }

  static void _showErrorToast(String message) {
    BotToast.showText(
      text: message,
      duration: Duration(seconds: 4),
      contentColor: Colors.red,
      textStyle: TextStyle(color: Colors.white, fontSize: 14),
    );
  }

  static void _showInfoToast(String message) {
    BotToast.showText(
      text: message,
      duration: Duration(seconds: 2),
      contentColor: Colors.blue,
      textStyle: TextStyle(color: Colors.white, fontSize: 14),
    );
  }
}
