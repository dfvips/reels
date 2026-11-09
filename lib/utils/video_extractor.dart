import 'package:douyin_parser/services/http_service.dart';
import 'package:douyin_parser/services/douyin_api_service.dart';
import 'package:douyin_parser/utils/url_parser.dart';

class VideoExtractor {
  /// 批量提取多个视频URL
  static Future<List<ExtractionResult>> extractMultipleVideoUrls(
    String input,
  ) async {
    final urls = UrlParser.extractUrls(input);
    final results = <ExtractionResult>[];

    print('找到 ${urls.length} 个链接需要解析');

    for (var i = 0; i < urls.length; i++) {
      var url = urls[i];
      print('正在解析第 ${i + 1}/${urls.length} 个链接: $url');

      results.add(await _extractOne(url));
    }

    return results;
  }

  static Future<ExtractionResult> _extractOne(String url) async {
    try {
      // 短链需要先跟随重定向拿到真实页面地址
      var pageUrl = url;
      var awemeId = UrlParser.extractAwemeId(url);
      if (awemeId == null && url.contains('v.douyin.com')) {
        var current = url;
        for (var hop = 0; hop < 5 && awemeId == null; hop++) {
          final redirected = await HttpService.getRedirectUrl(current);
          if (redirected == null || redirected.isEmpty) break;
          current = redirected.startsWith('http')
              ? redirected
              : Uri.parse(current).resolve(redirected).toString();
          pageUrl = current;
          awemeId = UrlParser.extractAwemeId(current);
        }
      }

      if (awemeId == null) {
        return ExtractionResult(
          success: false,
          originalUrl: url,
          error: '链接无效或格式不正确',
        );
      }

      // 首选：签名接口（不依赖页面结构）
      final detail = await DouyinApiService.fetchAwemeDetail(awemeId);
      if (detail != null) {
        return ExtractionResult(
          success: true,
          originalUrl: url,
          videoId: detail.playUri,
          videoTitle: detail.desc,
          videoUrl: detail.playUrls.first,
        );
      }

      // 回退：分享页 HTML 流程
      final html = await HttpService.fetchHtml(pageUrl);
      final videoId = UrlParser.extractVideoId(html);
      if (videoId == null) {
        return ExtractionResult(
          success: false,
          originalUrl: url,
          videoId: awemeId,
          error: '无法获取视频播放链接',
        );
      }

      final videoTitle = UrlParser.extractVideoTitle(html);
      final finalVideoUrl = await HttpService.getFinalVideoUrl(videoId);
      if (finalVideoUrl == null) {
        return ExtractionResult(
          success: false,
          originalUrl: url,
          videoId: awemeId,
          videoTitle: videoTitle,
          error: '无法获取视频播放链接',
        );
      }

      return ExtractionResult(
        success: true,
        originalUrl: url,
        videoId: awemeId,
        videoTitle: videoTitle,
        videoUrl: finalVideoUrl,
      );
    } catch (e) {
      print('解析链接失败: $e');
      return ExtractionResult(
        success: false,
        originalUrl: url,
        error: '解析失败: $e',
      );
    }
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
