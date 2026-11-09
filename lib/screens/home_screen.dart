import 'dart:async';
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
import 'package:window_manager/window_manager.dart';
import 'package:douyin_parser/widgets/desktop_title_bar.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  List<String> _results = [];
  bool _isLoading = false;
  List<String> _videoUrls = [];
  List<String> _videoTitles = []; // 新增：存储视频标题

  // 下载相关状态
  bool _isDownloading = false;
  int _currentDownloadIndex = 0;
  int _totalDownloads = 0;
  List<int> _downloadedBytes = [];
  List<int> _totalBytes = [];
  List<bool> _downloadCompleted = [];
  CancelToken? _downloadCancelToken;

  // 用于控制进度 dialog
  bool _isProgressDialogVisible = false;
  Completer<void>? _progressDialogCompleter; // 用于跟踪进度对话框的完成

  void _parseUrl(String text) async {
    setState(() {
      _isLoading = true;
      _results.clear();
      _videoUrls.clear();
      _videoTitles.clear(); // 清空标题列表
    });

    try {
      final results = await VideoExtractor.extractMultipleVideoUrls(text);

      setState(() {
        _isLoading = false;

        final successfulResults =
            results
                .where((result) => result.success && result.videoUrl != null)
                .toList();

        if (successfulResults.isNotEmpty) {
          _videoUrls =
              successfulResults.map((result) => result.videoUrl!).toList();
          _videoTitles =
              successfulResults.map((result) {
                // 使用视频标题，如果没有则使用默认名称
                return result.videoTitle ?? '抖音视频_${_videoTitles.length + 1}';
              }).toList();
          _results = _videoUrls;

          // 初始化下载状态
          _downloadedBytes = List.filled(_videoUrls.length, 0);
          _totalBytes = List.filled(_videoUrls.length, 0);
          _downloadCompleted = List.filled(_videoUrls.length, false);

          _showSuccessToast('成功解析 ${_results.length} 个视频链接');
        } else {
          final errors =
              results
                  .where((result) => result.error != null)
                  .map((result) => result.error!)
                  .toList();

          if (errors.isNotEmpty) {
            _showErrorToast('解析失败: ${errors.join(', ')}');
          } else {
            _showErrorToast('未找到有效的视频链接');
          }
        }
      });
    } catch (e) {
      setState(() {
        _isLoading = false;
        _showErrorToast('解析失败: $e');
      });
    }
  }

  void _showSuccessToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 3),
      contentColor: Colors.green,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
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

  Future<void> _copyLinks() async {
    if (_results.isNotEmpty) {
      // 按"标题\n链接"的格式复制
      final allLinks = [];
      for (int i = 0; i < _videoUrls.length; i++) {
        final title = _videoTitles[i];
        final link = _videoUrls[i];
        allLinks.add('$title\n$link');
      }
      final formattedText = allLinks.join('\n\n');
      await ClipboardUtils.copyToClipboard(formattedText);
    } else {
      _showErrorToast('没有可复制的内容');
    }
  }

  void _handleDownload() {
    if (!_isDownloading) {
      _downloadVideos();
    }
  }

  Future<bool> _requestPermissions() async {
    try {
      print('HomeScreen: 开始请求权限...');

      if (Platform.isAndroid || Platform.isIOS) {
        final hasPermission = await DownloadService.requestStoragePermission();
        print('HomeScreen: 权限请求结果: $hasPermission');
        return hasPermission;
      }

      return true;
    } catch (e) {
      print('HomeScreen: 权限请求失败: $e');
      _showErrorToast('权限请求失败，请检查系统设置');
      return false;
    }
  }

  Future<void> _downloadVideos() async {
    if (_videoUrls.isEmpty) {
      _showErrorToast('没有可下载的视频链接');
      return;
    }

    if (_isDownloading) {
      _showErrorToast('当前正在下载中，请等待完成');
      return;
    }

    // 移动端：检查并请求权限
    if (Platform.isAndroid || Platform.isIOS) {
      print('HomeScreen: 检查下载权限...');
      final hasPermission = await _requestPermissions();
      print('HomeScreen: 权限检查结果: $hasPermission');

      if (!hasPermission) {
        _showErrorToast('未授予存储权限，无法下载');
        _showSettingsDialog();
        return;
      }

      final shouldContinue = await _showBatchDownloadConfirmation();
      if (!shouldContinue) {
        return;
      }
    } else {
      final shouldContinue = await _showBatchDownloadConfirmation();
      if (!shouldContinue) {
        return;
      }
    }

    // 生成基础文件名（批量下载时使用）
    final baseFileName = DownloadService.generateFileName();
    _downloadCancelToken = CancelToken();

    setState(() {
      _isDownloading = true;
      _currentDownloadIndex = 0;
      _totalDownloads = _videoUrls.length;
      _downloadedBytes = List.filled(_videoUrls.length, 0);
      _totalBytes = List.filled(_videoUrls.length, 0);
      _downloadCompleted = List.filled(_videoUrls.length, false);
    });

    // 显示批量下载进度 dialog
    _showBatchDownloadProgressDialog();

    try {
      // 开始批量顺序下载
      final filePaths = await DownloadService.downloadMultipleVideos(
        _videoUrls,
        _videoTitles, // 传递视频标题列表
        baseFileName,
        (index, received, total) {
          if (mounted) {
            setState(() {
              _downloadedBytes[index] = received;
              _totalBytes[index] = total;
              // 更新当前下载索引（总是显示下一个要下载的索引）
              if (received < total || total == 0) {
                _currentDownloadIndex = index;
              } else {
                // 当前文件下载完成，标记为完成
                _downloadCompleted[index] = true;
                // 如果当前文件完成且不是最后一个，显示下一个索引
                if (index < _videoUrls.length - 1) {
                  _currentDownloadIndex = index + 1;
                }
              }
            });
          }
        },
        _downloadCancelToken,
        context,
      );

      // 所有下载完成后，显示完成通知
      final successfulDownloads =
          filePaths.where((path) => path != null).toList();

      if (successfulDownloads.isNotEmpty) {
        // 移动端：批量保存到相册
        if (Platform.isIOS || Platform.isAndroid) {
          await Future.delayed(Duration(seconds: 1));
          await DownloadService.saveMultipleVideosToGallery(
            successfulDownloads.cast<String>(),
          );

          // 移动端：显示完成提示
          _showSuccessToast(
            '批量下载完成: ${successfulDownloads.length}/$_totalDownloads 个视频',
          );
        } else {
          // 桌面端：显示统一的完成通知
          _showSuccessToast(
            '批量下载完成: ${successfulDownloads.length}/$_totalDownloads 个视频',
          );

          // 等待进度对话框关闭后再显示下载完成总结
          if (mounted && successfulDownloads.length > 0) {
            // 先等待进度对话框关闭完成
            if (_progressDialogCompleter != null) {
              await _progressDialogCompleter!.future;
            }

            // 然后显示总结对话框
            if (mounted) {
              await _showDownloadCompleteSummary(
                successfulDownloads.cast<String>(),
              );
            }
          }
        }
      } else {
        _showErrorToast('下载失败: 0/$_totalDownloads 个视频');
      }
    } catch (e) {
      print('HomeScreen: 批量下载异常: $e');
      _showErrorToast('批量下载失败: $e');
    } finally {
      if (mounted) {
        setState(() {
          _isDownloading = false;
          _downloadCancelToken = null;
        });
      }
      _cancelDownload();
      // Navigator.of(context).pop();
    }
  }

  Future<void> _showDownloadCompleteSummary(List<String> filePaths) async {
    if (filePaths.isEmpty) return;

    await showDialog<void>(
      context: context,
      barrierDismissible: false, // 设置为false，不允许点击外部关闭
      builder: (context) {
        return AlertDialog(
          title: const Text('下载完成'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '已成功下载 ${filePaths.length} 个视频文件：',
                  style: const TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const SizedBox(height: 12),
                ...filePaths.take(5).map((path) {
                  final fileName = path.split('/').last;
                  return Padding(
                    padding: const EdgeInsets.symmetric(vertical: 2.0),
                    child: Text(
                      '• $fileName',
                      style: const TextStyle(fontSize: 12),
                      overflow: TextOverflow.ellipsis,
                    ),
                  );
                }).toList(),
                if (filePaths.length > 5) ...[
                  const SizedBox(height: 4),
                  Text(
                    '... 以及 ${filePaths.length - 5} 个文件',
                    style: const TextStyle(fontSize: 12, color: Colors.grey),
                  ),
                ],
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
              },
              child: const Text('关闭'),
            ),
            if (Platform.isMacOS)
              TextButton(
                onPressed: () async {
                  try {
                    final firstFile = File(filePaths.first);
                    if (await firstFile.exists()) {
                      final directory = firstFile.parent.path;
                      await Process.run('open', [directory]);
                    }
                  } catch (e) {
                    print('打开目录失败: $e');
                  }
                  // 注意：不关闭对话框，让用户自己点击关闭
                },
                child: const Text('打开文件夹'),
              ),
            if (Platform.isWindows)
              TextButton(
                onPressed: () async {
                  try {
                    final firstFile = File(filePaths.first);
                    if (await firstFile.exists()) {
                      final directory = firstFile.parent.path;
                      await Process.run('explorer', [directory]);
                    }
                  } catch (e) {
                    print('打开目录失败: $e');
                  }
                  // 注意：不关闭对话框，让用户自己点击关闭
                },
                child: const Text('打开文件夹'),
              ),
          ],
        );
      },
    );
  }

  void _showSettingsDialog() {
    showDialog(
      context: context,
      barrierDismissible: true,
      builder: (context) {
        return AlertDialog(
          title: const Text('权限被拒绝'),
          content: const Text('请在系统设置中授予存储权限以继续下载'),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('取消'),
            ),
            ElevatedButton(
              onPressed: () {
                Navigator.of(context).pop();
                openAppSettings();
              },
              child: const Text('去设置'),
            ),
          ],
        );
      },
    );
  }

  Future<bool> _showBatchDownloadConfirmation() async {
    final isDarkMode = Theme.of(context).brightness == Brightness.dark;
    final primaryColor = Theme.of(context).colorScheme.primary;
    final onPrimaryColor = Theme.of(context).colorScheme.onPrimary;

    return await showDialog<bool>(
          context: context,
          builder: (context) {
            return AlertDialog(
              title: const Text('下载确认'),
              content: Text(
                '视频将按顺序依次下载，下载完成后${Platform.isAndroid || Platform.isIOS ? '将自动保存到相册' : '将保存到您选择的位置'}。\n确定要顺序下载这 ${_videoUrls.length} 个视频吗？',
              ),
              actions: [
                TextButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: const Text('取消'),
                ),

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
                      side: BorderSide.none,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(16),
                      ),
                      elevation: 0,
                    ),
                    child: const Text('开始下载'),
                  ),
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
    if (Navigator.of(context).canPop()) {
      Navigator.of(context).pop();
    }
  }

  void _showInfoToast(String message) {
    BotToast.showText(
      text: message,
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
  }

  // 显示批量下载进度 dialog
  void _showBatchDownloadProgressDialog() {
    if (_isProgressDialogVisible) {
      return;
    }

    _isProgressDialogVisible = true;
    _progressDialogCompleter = Completer<void>();

    showDialog(
      context: context,
      barrierDismissible: false, // 不允许点击外部关闭
      builder: (context) {
        return StatefulBuilder(
          builder: (context, setState) {
            // 保存对话框的 StateSetter 以便更新
            return AlertDialog(
              title: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text('下载进度'),
                  IconButton(
                    icon: const Icon(Icons.close, size: 20),
                    onPressed: () {
                      _cancelDownload();
                      // Navigator.of(context).pop(); // 直接关闭对话框
                    },
                    tooltip: '关闭',
                  ),
                ],
              ),
              content: SizedBox(
                width: double.infinity,
                child: StreamBuilder(
                  stream: Stream.periodic(const Duration(milliseconds: 200)),
                  builder: (context, snapshot) {
                    // 计算已完成的数量
                    int completedCount =
                        _downloadCompleted
                            .where((completed) => completed)
                            .length;

                    // 计算总体进度
                    double overallProgress = 0.0;

                    if (_totalDownloads > 0) {
                      // 已完成的文件贡献的进度
                      double completedProgress =
                          completedCount / _totalDownloads;

                      // 当前文件的进度贡献
                      double currentFileProgress = 0.0;
                      if (_currentDownloadIndex < _totalBytes.length &&
                          _totalBytes[_currentDownloadIndex] > 0 &&
                          !_downloadCompleted[_currentDownloadIndex]) {
                        currentFileProgress =
                            (_downloadedBytes[_currentDownloadIndex] /
                                _totalBytes[_currentDownloadIndex]) /
                            _totalDownloads;
                      }

                      overallProgress = completedProgress + currentFileProgress;
                    }

                    overallProgress = overallProgress.clamp(0.0, 1.0);

                    // 当前视频进度
                    final currentProgress =
                        _currentDownloadIndex < _totalBytes.length &&
                                _totalBytes[_currentDownloadIndex] > 0 &&
                                !_downloadCompleted[_currentDownloadIndex]
                            ? (_downloadedBytes[_currentDownloadIndex] /
                                    _totalBytes[_currentDownloadIndex])
                                .clamp(0.0, 1.0)
                            : _currentDownloadIndex <
                                    _downloadCompleted.length &&
                                _downloadCompleted[_currentDownloadIndex]
                            ? 1.0
                            : 0.0;

                    final overallPercentage = (overallProgress * 100)
                        .toStringAsFixed(1);
                    final currentPercentage = (currentProgress * 100)
                        .toStringAsFixed(1);

                    // 计算总下载字节数
                    int totalDownloaded = 0;
                    int totalSize = 0;

                    for (var i = 0; i < _downloadedBytes.length; i++) {
                      totalDownloaded += _downloadedBytes[i];
                      if (_totalBytes[i] > 0) {
                        totalSize += _totalBytes[i];
                      } else if (_downloadCompleted[i]) {
                        // 如果文件已下载完成但_totalBytes为0，使用_downloadedBytes作为总大小
                        totalSize += _downloadedBytes[i];
                      }
                    }

                    final downloadedSize = DownloadService.formatFileSize(
                      totalDownloaded,
                    );
                    final totalDownloadSize =
                        totalSize > 0
                            ? DownloadService.formatFileSize(totalSize)
                            : '计算中...';

                    String statusText = '正在顺序下载...';
                    bool allCompleted =
                        completedCount == _totalDownloads &&
                        _totalDownloads > 0;

                    if (allCompleted) {
                      statusText = '所有下载完成！正在关闭进度窗口...';

                      // 所有下载完成后，立即关闭进度对话框
                      Future.delayed(const Duration(milliseconds: 300), () {
                        if (mounted && _isProgressDialogVisible) {
                          Navigator.of(context).pop();
                          if (_progressDialogCompleter != null &&
                              !_progressDialogCompleter!.isCompleted) {
                            _progressDialogCompleter!.complete();
                          }
                        }
                      });
                    }

                    return Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        // 总体进度
                        Row(
                          children: [
                            const Text('总体进度:', style: TextStyle(fontSize: 12)),
                            const SizedBox(width: 8),
                            Text(
                              '$completedCount/$_totalDownloads 已完成',
                              style: const TextStyle(
                                fontSize: 12,
                                color: Colors.green,
                              ),
                            ),
                          ],
                        ),
                        const SizedBox(height: 4),
                        LinearProgressIndicator(
                          value: overallProgress,
                          backgroundColor: Colors.grey[200],
                          valueColor: AlwaysStoppedAnimation<Color>(
                            Theme.of(context).colorScheme.primary,
                          ),
                        ),
                        const SizedBox(height: 4),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            Text(
                              '$overallPercentage%',
                              style: const TextStyle(
                                fontSize: 12,
                                fontWeight: FontWeight.bold,
                              ),
                            ),
                            Text(
                              '$downloadedSize / $totalDownloadSize',
                              style: const TextStyle(fontSize: 12),
                            ),
                          ],
                        ),

                        const SizedBox(height: 12),

                        // 当前视频进度
                        if (_currentDownloadIndex < _totalDownloads)
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                '当前视频 (${_currentDownloadIndex + 1}/$_totalDownloads):',
                                style: const TextStyle(fontSize: 12),
                              ),
                              const SizedBox(height: 4),
                              LinearProgressIndicator(
                                value: currentProgress,
                                backgroundColor: Colors.grey[200],
                                valueColor: AlwaysStoppedAnimation<Color>(
                                  Theme.of(context).colorScheme.secondary,
                                ),
                              ),
                              const SizedBox(height: 4),
                              Row(
                                mainAxisAlignment:
                                    MainAxisAlignment.spaceBetween,
                                children: [
                                  Text(
                                    '$currentPercentage%',
                                    style: const TextStyle(
                                      fontSize: 12,
                                      fontWeight: FontWeight.bold,
                                    ),
                                  ),
                                  if (_currentDownloadIndex <
                                          _totalBytes.length &&
                                      (_totalBytes[_currentDownloadIndex] > 0 ||
                                          _downloadCompleted[_currentDownloadIndex]))
                                    Text(
                                      '${DownloadService.formatFileSize(_downloadedBytes[_currentDownloadIndex])} / '
                                      '${_totalBytes[_currentDownloadIndex] > 0 ? DownloadService.formatFileSize(_totalBytes[_currentDownloadIndex]) : DownloadService.formatFileSize(_downloadedBytes[_currentDownloadIndex])}',
                                      style: const TextStyle(fontSize: 12),
                                    ),
                                ],
                              ),
                            ],
                          ),

                        const SizedBox(height: 12),
                        Text(
                          statusText,
                          style: TextStyle(
                            fontSize: 11,
                            color: allCompleted ? Colors.green : Colors.grey,
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
            );
          },
        );
      },
    ).then((_) {
      // dialog 被关闭后的回调
      if (mounted) {
        setState(() {
          _isProgressDialogVisible = false;
        });
      }

      // 完成 completer
      if (_progressDialogCompleter != null &&
          !_progressDialogCompleter!.isCompleted) {
        _progressDialogCompleter!.complete();
      }
    });
  }

  void _showSupportedFormats() {
    showDialog(
      context: context,
      barrierDismissible: true,
      builder: (context) {
        return AlertDialog(
          title: const Text(
            '使用说明',
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
          ),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text('支持解析以下链接格式：', style: TextStyle(fontSize: 14)),
                const SizedBox(height: 12),
                _buildFormatItem(
                  '5.12 复制打开抖音，看看【赵今麦angel的作品】小林～拍照📷# 剧剧抖有戏  https://v.douyin.com/HUbJB4-GQdc/ 03/26 e@b.nD yGi:/ ',
                ),
                _buildFormatItem('https://v.douyin.com/L8tSCYe-sC4'),
                _buildFormatItem(
                  'https://www.douyin.com/video/7588361322312650038',
                ),
                _buildFormatItem(
                  'https://m.douyin.com/share/video/7588361322312650038',
                ),
                _buildFormatItem(
                  'https://www.iesdouyin.com/share/video/7588361322312650038',
                ),
                _buildFormatItem(
                  'https://www.douyin.com/jingxuan?modal_id=7701193143558901018',
                ),
                const SizedBox(height: 16),
                Container(
                  padding: const EdgeInsets.all(0),
                  child: Row(
                    children: [
                      Transform.rotate(
                        angle: -45 * (3.14159 / 180),
                        child: Icon(Icons.link, size: 16, color: Colors.blue),
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          '支持批量粘贴多个抖音分享链接，系统会自动提取所有有效链接。',
                          style: TextStyle(
                            fontSize: 13,
                            color: Colors.grey[600],
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
              },
              child: const Text('知道了'),
            ),
          ],
        );
      },
    );
  }

  Widget _buildFormatItem(String url) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4.0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Padding(
            padding: EdgeInsets.only(top: 2.0, right: 8.0),
            child: Icon(Icons.check_circle, size: 16, color: Colors.green),
          ),
          Expanded(
            child: SelectableText(
              url,
              style: const TextStyle(
                fontSize: 13,
                fontFamily: 'Monospace',
                color: Colors.blue,
              ),
            ),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final scaffold = Scaffold(
      appBar: _buildAppBar(),
      body: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            UrlInput(onParse: _parseUrl),

            if (_isLoading)
              Column(
                children: [
                  const SizedBox(height: 24),
                  const CircularProgressIndicator(),
                  const SizedBox(height: 16),
                  Text(
                    '正在解析链接...',
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.secondary,
                      fontSize: 16,
                    ),
                  ),
                ],
              )
            else if (_results.isNotEmpty)
              ResultDisplay(
                results: _results,
                titles: _videoTitles,
                onCopy: _copyLinks,
                onDownload: _handleDownload,
              ),
            ],
          ),
        ),
      );

    // Windows 无边框窗口需要自绘边缘拖拽缩放
    if (Platform.isWindows) {
      return DragToResizeArea(
        resizeEdgeSize: 8,
        child: scaffold,
      );
    }
    return scaffold;
  }

  PreferredSizeWidget _buildAppBar() {
    final title = Row(
      children: [
        Icon(Icons.play_circle_filled, color: Color(0xFF3586F9), size: 26),
        SizedBox(width: 4),
        const Text(
          '抖音视频解析',
          style: TextStyle(
            color: Color(0xFF3586F9),
            fontSize: 20,
            fontWeight: FontWeight.bold,
          ),
        ),
      ],
    );
    final actions = [
      IconButton(
        icon: const Icon(Icons.contact_support),
        color: const Color(0xFF3586F9),
        onPressed: _showSupportedFormats,
        tooltip: '帮助',
      ),
      Padding(
        padding: const EdgeInsets.only(right: 10),
        child: IconButton(
          icon: const Icon(Icons.settings),
          color: const Color(0xFF3586F9),
          onPressed: () {
            Navigator.pushNamed(context, '/settings');
          },
          tooltip: '设置',
        ),
      ),
    ];

    if (Platform.isWindows || Platform.isMacOS) {
      return PreferredSize(
        preferredSize: const Size.fromHeight(DesktopTitleBar.totalHeight),
        child: DesktopTitleBar(title: title, actions: actions),
      );
    }

    return AppBar(
      title: title,
      actions: actions,
    );
  }

  @override
  void dispose() {
    if (_downloadCancelToken != null && !_downloadCancelToken!.isCancelled) {
      _downloadCancelToken!.cancel();
    }

    // 完成 completer 避免内存泄漏
    if (_progressDialogCompleter != null &&
        !_progressDialogCompleter!.isCompleted) {
      _progressDialogCompleter!.complete();
    }

    super.dispose();
  }
}
