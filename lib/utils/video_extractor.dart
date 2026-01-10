import 'package:douyin_parser/services/http_service.dart';
import 'package:douyin_parser/utils/url_parser.dart';

class VideoExtractor {
  /// 提取单个视频URL
  static Future<ExtractionResult> extractVideoUrl(String input) async {
    try {
      // 提取URL
      final urls = UrlParser.extractUrls(input);
      if (urls.isEmpty) {
        return ExtractionResult(success: false, error: '链接无效或格式不正确');
      }

      final url = urls.first;
      print('提取到的URL: $url');

      // 获取HTML
      final html = await HttpService.fetchHtml(url);

      // 提取视频ID
      final videoId = UrlParser.extractVideoId(html);

      if (videoId == null) {
        return ExtractionResult(
          success: false,
          error: '无法从页面中提取视频ID',
          originalUrl: url,
        );
      }

      print('提取到的视频ID: $videoId');

      // 提取视频标题
      final videoTitle = UrlParser.extractVideoTitle(html);
      print('提取到的视频标题: $videoTitle');

      // 生成视频URL并获取最终重定向链接
      final finalVideoUrl = await HttpService.getFinalVideoUrl(videoId);

      if (finalVideoUrl == null) {
        return ExtractionResult(
          success: false,
          error: '无法获取视频播放链接',
          originalUrl: url,
          videoId: videoId,
          videoTitle: videoTitle,
        );
      }

      print('最终视频URL: $finalVideoUrl');

      return ExtractionResult(
        success: true,
        originalUrl: url,
        videoId: videoId,
        videoTitle: videoTitle,
        videoUrl: finalVideoUrl,
      );
    } catch (e) {
      print('提取视频失败: $e');
      return ExtractionResult(success: false, error: '解析失败: $e');
    }
  }

  /// 批量提取多个视频URL
  static Future<List<ExtractionResult>> extractMultipleVideoUrls(
    String input,
  ) async {
    final urls = UrlParser.extractUrls(input);
    final results = <ExtractionResult>[];

    print('找到 ${urls.length} 个链接需要解析');

    for (var i = 0; i < urls.length; i++) {
      final url = urls[i];
      print('正在解析第 ${i + 1}/${urls.length} 个链接: $url');

      try {
        // 获取HTML
        final html = await HttpService.fetchHtml(url);

        // 提取视频ID
        final videoId = UrlParser.extractVideoId(html);

        if (videoId == null) {
          results.add(
            ExtractionResult(
              success: false,
              originalUrl: url,
              error: '无法从页面中提取视频ID',
            ),
          );
          continue;
        }

        print('第 ${i + 1} 个链接提取到的视频ID: $videoId');

        // 提取视频标题
        final videoTitle = UrlParser.extractVideoTitle(html);
        print('第 ${i + 1} 个链接提取到的视频标题: $videoTitle');

        // 生成视频URL并获取最终重定向链接
        final finalVideoUrl = await HttpService.getFinalVideoUrl(videoId);

        if (finalVideoUrl == null) {
          results.add(
            ExtractionResult(
              success: false,
              originalUrl: url,
              videoId: videoId,
              videoTitle: videoTitle,
              error: '无法获取视频播放链接',
            ),
          );
          continue;
        }

        print('第 ${i + 1} 个链接最终视频URL: $finalVideoUrl');

        results.add(
          ExtractionResult(
            success: true,
            originalUrl: url,
            videoId: videoId,
            videoTitle: videoTitle,
            videoUrl: finalVideoUrl,
          ),
        );
      } catch (e) {
        print('解析第 ${i + 1} 个链接失败: $e');
        results.add(
          ExtractionResult(success: false, originalUrl: url, error: '解析失败: $e'),
        );
      }
    }

    return results;
  }
}

class ExtractionResult {
  final bool success;
  final String? originalUrl;
  final String? videoId;
  final String? videoTitle; // 新增：视频标题
  final String? videoUrl;
  final String? error;

  ExtractionResult({
    required this.success,
    this.originalUrl,
    this.videoId,
    this.videoTitle,
    this.videoUrl,
    this.error,
  });
}
