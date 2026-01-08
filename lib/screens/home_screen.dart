import 'dart:io';
import 'package:flutter/material.dart';
import 'package:douyin_parser/utils/clipboard_utils.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:douyin_parser/utils/video_extractor.dart';
import 'package:douyin_parser/widgets/url_input.dart';
import 'package:douyin_parser/widgets/result_display.dart';
import 'package:bot_toast/bot_toast.dart';
import 'package:dio/dio.dart';
import 'package:permission_handler/permission_handler.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  String? _result;
  bool _isLoading = false;
  String? _videoUrl;

  // 下载相关状态
  bool _isDownloading = false;
  int _downloadedBytes = 0;
  int _totalBytes = 0;
  CancelToken? _downloadCancelToken;

  void _parseUrl(String text) async {
    setState(() {
      _isLoading = true;
      _result = null;
      _videoUrl = null;
    });

    try {
      final result = await VideoExtractor.extractVideoUrl(text);

      setState(() {
        _isLoading = false;
        if (result.success) {
          _videoUrl = result.videoUrl;
          _result = result.videoUrl ?? '解析成功但未获取到链接';
        } else {
          _showErrorToast(result.error ?? '解析失败');
        }
      });
    } catch (e) {
      setState(() {
        _isLoading = false;
        _showErrorToast('解析失败: $e');
      });
    }
  }

  void _showErrorToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 4),
      contentColor: Colors.red[800]?.withOpacity(0.9) ?? Colors.red,
      textStyle: const TextStyle(
        color: Colors.white,
        fontSize: 16.0,
        fontWeight: FontWeight.w500,
      ),
      borderRadius: BorderRadius.circular(8),
    );
  }

  Future<void> _copyLink() async {
    if (_result != null && _result!.isNotEmpty) {
      await ClipboardUtils.copyToClipboard(_result!);
    } else {
      _showErrorToast('没有可复制的内容');
    }
  }

  void _handleDownload() {
    if (!_isDownloading) {
      _downloadVideo();
    }
  }

  // 添加权限请求方法
  Future<bool> _requestPermissions() async {
    try {
      print('HomeScreen: 开始请求权限...');

      if (Platform.isAndroid || Platform.isIOS) {
        // 直接使用 DownloadService 的权限请求方法
        final hasPermission = await DownloadService.requestStoragePermission();
        print('HomeScreen: 权限请求结果: $hasPermission');
        return hasPermission;
      }

      return true; // 桌面平台不需要特殊权限
    } catch (e) {
      print('HomeScreen: 权限请求失败: $e');
      _showErrorToast('权限请求失败，请检查系统设置');
      return false;
    }
  }

  Future<void> _downloadVideo() async {
    if (_videoUrl == null || _videoUrl!.isEmpty) {
      _showErrorToast('没有可下载的视频链接');
      return;
    }

    if (_isDownloading) {
      _showErrorToast('当前正在下载中，请等待完成');
      return;
    }

    // 检查并请求权限（仅移动端）
    if (Platform.isAndroid || Platform.isIOS) {
      print('HomeScreen: 检查下载权限...');
      final hasPermission = await _requestPermissions();
      print('HomeScreen: 权限检查结果: $hasPermission');

      if (!hasPermission) {
        _showErrorToast('未授予存储权限，无法下载');
        _showSettingsDialog();
        return;
      }
    }

    final fileName = DownloadService.generateFileName();
    _downloadCancelToken = CancelToken();

    setState(() {
      _isDownloading = true;
      _downloadedBytes = 0;
      _totalBytes = 0;
    });

    try {
      // 移动端显示下载确认对话框
      if (Platform.isAndroid || Platform.isIOS) {
        final shouldContinue = await _showDownloadConfirmation();
        if (!shouldContinue) {
          setState(() {
            _isDownloading = false;
          });
          return;
        }
      }

      // 开始下载
      final filePath = await DownloadService.downloadVideo(
        _videoUrl!,
        fileName,
        (received, total) {
          if (mounted) {
            setState(() {
              _downloadedBytes = received;
              _totalBytes = total;
            });
          }
        },
        _downloadCancelToken,
        context, // 传递 context 给 DownloadService
      );

      // 移动端：下载完成后保存到相册
      if (filePath != null && (Platform.isIOS || Platform.isAndroid)) {
        await DownloadService.saveVideoToGallery(filePath);
      }
    } catch (e) {
      print('HomeScreen: 下载异常: $e');
      _showErrorToast('下载失败: $e');
    } finally {
      if (mounted) {
        setState(() {
          _isDownloading = false;
          _downloadCancelToken = null;
        });
      }
    }
  }

  // 显示去设置对话框
  void _showSettingsDialog() {
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

  Future<bool> _showDownloadConfirmation() async {
    return await showDialog<bool>(
          context: context,
          builder: (context) {
            return AlertDialog(
              title: const Text('下载视频'),
              content: const Text('确定要下载这个视频吗？'),
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
                  child: const Text('开始下载'),
                ),
              ],
            );
          },
        ) ??
        false;
  }

  void _cancelDownload() {
    if (_downloadCancelToken != null && !_downloadCancelToken!.isCancelled) {
      _downloadCancelToken!.cancel();
      setState(() {
        _isDownloading = false;
      });
      _showInfoToast('下载已取消');
    }
  }

  void _showInfoToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(
        color: Colors.white,
        fontSize: 16.0,
        fontWeight: FontWeight.w500,
      ),
      borderRadius: BorderRadius.circular(8),
    );
  }

  // 下载进度显示组件
  Widget _buildDownloadProgress() {
    if (!_isDownloading) return const SizedBox.shrink();

    // 添加空值检查
    final progress =
        _totalBytes > 0
            ? (_downloadedBytes / _totalBytes).clamp(0.0, 1.0)
            : 0.0;

    final downloadedSize = DownloadService.formatFileSize(_downloadedBytes);
    final totalSize = DownloadService.formatFileSize(_totalBytes);
    final percentage = (progress * 100).toStringAsFixed(1);

    return Card(
      margin: const EdgeInsets.only(bottom: 16),
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text(
                  '正在下载视频...',
                  style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
                ),
                IconButton(
                  icon: const Icon(Icons.close),
                  onPressed: _cancelDownload,
                  tooltip: '取消下载',
                ),
              ],
            ),
            const SizedBox(height: 12),
            LinearProgressIndicator(
              value: progress,
              backgroundColor: Colors.grey[200],
              valueColor: AlwaysStoppedAnimation<Color>(
                Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  '$percentage%',
                  style: const TextStyle(fontWeight: FontWeight.bold),
                ),
                Text('$downloadedSize / $totalSize'),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              Platform.isMacOS ? '正在下载到用户选择的位置...' : '下载完成后将自动提示保存',
              style: const TextStyle(fontSize: 12, color: Colors.grey),
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('抖音视频解析工具'),
        centerTitle: true,
        actions: [
          IconButton(
            icon: const Icon(Icons.settings),
            onPressed: () {
              Navigator.pushNamed(context, '/settings');
            },
          ),
        ],
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // 输入区域
            UrlInput(onParse: _parseUrl),

            const SizedBox(height: 24),

            // 下载进度显示
            _buildDownloadProgress(),

            // 解析状态
            if (_isLoading)
              Column(
                children: [
                  const CircularProgressIndicator(),
                  const SizedBox(height: 16),
                  Text(
                    '正在解析中...',
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.secondary,
                      fontSize: 16,
                    ),
                  ),
                ],
              )
            else if (_result != null)
              ResultDisplay(
                result: _result!,
                onCopy: _copyLink,
                onDownload: _handleDownload,
              ),
          ],
        ),
      ),
    );
  }

  @override
  void dispose() {
    // 清理下载任务
    if (_downloadCancelToken != null && !_downloadCancelToken!.isCancelled) {
      _downloadCancelToken!.cancel();
    }
    super.dispose();
  }
}
