import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:path/path.dart' as path;
import 'package:path_provider/path_provider.dart';

import 'package:douyin_parser/services/download_service.dart';

/// 导出第三方下载工具脚本，行为对齐 yspvod_go 项目：
/// 依次为 aria2c / downloader / RobustDownloader 各导出一份脚本，
/// Windows 端为 .bat（chcp 65001 + CRLF），macOS 端为 .sh（LF）。
class ScriptExportService {
  static const String _referer = 'https://www.douyin.com/';
  static const String _windowsUserAgent =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
  static const String _macUserAgent =
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';

  static const List<String> _tools = [
    'aria2c',
    'downloader',
    'RobustDownloader',
  ];

  /// 导出全部工具的脚本，返回生成的文件路径列表。
  static Future<List<String>> exportAll({
    required List<String> urls,
    required List<String> titles,
  }) async {
    if (urls.isEmpty) return [];

    final isWindows = Platform.isWindows;
    final outputDirPath = await _resolveOutputDirectory();
    if (outputDirPath == null) return [];
    final outputDir = Directory(outputDirPath);
    final baseName = buildScriptBaseName(titles, count: urls.length);
    final paths = <String>[];

    for (final tool in _tools) {
      final content = _buildScript(tool, urls, titles, isWindows);
      final extension = isWindows ? '.bat' : '.sh';
      final file = File(
        path.join(outputDir.path, '${baseName}_$tool$extension'),
      );
      await file.writeAsString(content, flush: true);
      if (!isWindows) {
        try {
          await Process.run('chmod', ['+x', file.path]);
        } catch (_) {
          // 无 chmod 时忽略，脚本仍可用 bash 手动执行
        }
      }
      paths.add(file.path);
    }

    return paths;
  }

  /// 脚本文件名规则：第一个标题前 10 个字 + "..."，
  /// 多个视频时再拼「 - 最后一个标题前 10 个字 + "..."」。
  static String buildScriptBaseName(
    List<String> titles, {
    required int count,
  }) {
    String shorten(String title) {
      final safe = DownloadService.safeFileName(title);
      final runes = safe.runes.toList();
      if (runes.length <= 10) return safe;
      return '${String.fromCharCodes(runes.take(10))}...';
    }

    final firstTitle = titles.isNotEmpty ? titles.first : '抖音视频';
    var baseName = shorten(firstTitle);
    if (count > 1) {
      final lastIndex = count - 1;
      final lastTitle =
          lastIndex < titles.length
              ? titles[lastIndex]
              : (titles.isNotEmpty ? titles.last : '抖音视频');
      baseName = '$baseName - ${shorten(lastTitle)}';
    }
    return baseName;
  }

  /// 解析导出目录，行为与下载保持一致：
  /// 优先复用已验证可写的上次目录；首次导出时弹出目录选择框，
  /// 由用户选择（如系统「下载」目录）并记住。
  ///
  /// 注意：macOS 沙盒下 getDownloadsDirectory() 返回容器内路径，
  /// 直接写入会报 Operation not permitted，必须走用户选择授权。
  static Future<String?> _resolveOutputDirectory() async {
    final lastDirectory = await DownloadService.getLastDirectory();
    if (lastDirectory.isNotEmpty &&
        await DownloadService.checkDirectoryAccess(lastDirectory)) {
      return lastDirectory;
    }

    String? initialDirectory;
    if (Platform.isWindows || Platform.isLinux) {
      try {
        initialDirectory = (await getDownloadsDirectory())?.path;
      } catch (_) {}
    }

    final picked = await FilePicker.platform.getDirectoryPath(
      dialogTitle: '选择下载脚本保存位置',
      initialDirectory: initialDirectory,
    );
    if (picked == null || picked.isEmpty) return null;
    if (!await DownloadService.checkDirectoryAccess(picked)) {
      throw Exception('没有该目录的写入权限，请重新选择保存位置');
    }
    await DownloadService.setLastDirectory(picked);
    return picked;
  }

  static String _buildScript(
    String tool,
    List<String> urls,
    List<String> titles,
    bool isWindows,
  ) {
    final lineEnd = isWindows ? '\r\n' : '\n';
    final buffer = StringBuffer();
    // cmd 默认按 OEM 代码页解析批处理，含中文时需先切换到 UTF-8
    buffer.write(isWindows ? 'chcp 65001 >nul' : '#!/bin/bash');
    buffer.write(lineEnd);
    buffer.write(lineEnd);

    final usedNames = <String>{};
    for (var i = 0; i < urls.length; i++) {
      final title = i < titles.length ? titles[i] : '抖音视频';
      var fileName = DownloadService.safeFileName(title, maxLength: 80);
      var uniqueName = fileName;
      var suffix = 2;
      while (!usedNames.add(uniqueName)) {
        uniqueName = '${fileName}_$suffix';
        suffix++;
      }
      fileName = uniqueName;

      buffer.write(_buildCommand(tool, urls[i], '$fileName.mp4', isWindows));
      buffer.write(lineEnd);
    }
    return buffer.toString();
  }

  static String _buildCommand(
    String tool,
    String url,
    String fileName,
    bool isWindows,
  ) {
    final userAgent = isWindows ? _windowsUserAgent : _macUserAgent;
    final headerArgs = [
      '--header',
      _quote('User-Agent:$userAgent', isWindows),
      '--header',
      _quote('Referer:$_referer', isWindows),
    ].join(' ');
    final quotedUrl = _quote(url, isWindows);
    final quotedName = _quote(fileName, isWindows);

    switch (tool) {
      case 'aria2c':
        return 'aria2c -c -s 16 -x 16 -k 1M -o $quotedName $quotedUrl $headerArgs';
      case 'downloader':
        return 'downloader $quotedUrl $quotedName 8 16 $headerArgs';
      case 'RobustDownloader':
        final executable = isWindows ? 'RobustDownloader.exe' : 'RobustDownloader';
        return '$executable --add $quotedUrl --dir "." --name $quotedName '
            '--threads 16 --block-size 1 $headerArgs --start --silent';
      default:
        return '';
    }
  }

  /// bat 用双引号（% 需转义为 %%）；sh 用单引号（内部单引号按 shell 规则拼接）。
  static String _quote(String value, bool isWindows) {
    if (isWindows) {
      return '"${value.replaceAll('%', '%%')}"';
    }
    return "'${value.replaceAll("'", "'\\''")}'";
  }
}
