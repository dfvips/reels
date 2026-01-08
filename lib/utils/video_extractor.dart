import 'package:douyin_parser/services/http_service.dart';
import 'package:douyin_parser/utils/url_parser.dart';

class VideoExtractor {
  static Future<ExtractionResult> extractVideoUrl(String input) async {
    try {
      // 提取URL
      final url = UrlParser.extractUrl(input);
      if (url == null) {
        return ExtractionResult(
          success: false,
          error: '未找到有效的URL',
        );
      }

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

      // 生成视频URL并获取最终重定向链接
      final finalVideoUrl = await HttpService.getFinalVideoUrl(videoId);

      if (finalVideoUrl == null) {
        return ExtractionResult(
          success: false,
          error: '无法获取视频播放链接',
          originalUrl: url,
          videoId: videoId,
        );
      }

      print('最终视频URL: $finalVideoUrl');

      return ExtractionResult(
        success: true,
        originalUrl: url,
        videoId: videoId,
        videoUrl: finalVideoUrl,
      );
    } catch (e) {
      print('提取视频失败: $e');
      return ExtractionResult(
        success: false,
        error: '解析失败: $e',
      );
    }
  }
}

class ExtractionResult {
  final bool success;
  final String? originalUrl;
  final String? videoId;
  final String? videoUrl;
  final String? error;

  ExtractionResult({
    required this.success,
    this.originalUrl,
    this.videoId,
    this.videoUrl,
    this.error,
  });
}
