class UrlParser {
  static String? extractUrl(String text) {
    // 改进的正则表达式：匹配URL直到遇到空格、标点符号或中文等非URL字符
    final regex = RegExp(
      r'(?:https?://v\.douyin\.com/[A-Za-z0-9_-]+|https?://(?:www\.|m\.)?douyin\.com/(?:video/[0-9]+|share/video/[0-9]+)|https?://www\.iesdouyin\.com/share/video/[0-9]+/)',
      caseSensitive: false,
    );

    final matches = regex.allMatches(text);
    if (matches.isNotEmpty) {
      if (matches.isNotEmpty) {
        final url = matches.first.group(0)!;
        // 调用convertDouyinVideoUrl进行转换
        return convertDouyinVideoUrl(url);
      }
      return null;
    }

    return null;
  }

  static String convertDouyinVideoUrl(String url) {
    // 匹配抖音视频链接模式
    final douyinPattern =
        RegExp(r'^https?://(?:www\.)?douyin\.com/video/(\d+)/?$');
    final match = douyinPattern.firstMatch(url);

    if (match != null) {
      final videoId = match.group(1);
      return 'https://www.iesdouyin.com/share/video/$videoId/';
    }

    // 如果没有匹配到，返回原链接
    return url;
  }

  static String? extractVideoId(String html) {
    // 方法1: 查找 video_id=v0200fg10000d57jnenog65ilc6e0r5g 格式（直接值）
    final regex1 = RegExp(r'video_id=([a-zA-Z0-9_\-]+)');
    final match1 = regex1.firstMatch(html);

    if (match1 != null) {
      final id = match1.group(1);
      print('方法1匹配到: $id');
      return id;
    }

    // 方法2: 查找 "video_id":"v0200fg10000d57jnenog65ilc6e0r5g" 格式（带双引号）
    final regex2 = RegExp(r'"video_id"\s*:\s*"([^"]+)"');
    final match2 = regex2.firstMatch(html);

    if (match2 != null) {
      final id = match2.group(1);
      print('方法2匹配到: $id');
      return id;
    }

    // 方法3: 查找 'video_id':'v0200fg10000d57jnenog65ilc6e0r5g' 格式（带单引号）
    final regex3 = RegExp(r"'video_id'\s*:\s*'([^']+)'");
    final match3 = regex3.firstMatch(html);

    if (match3 != null) {
      final id = match3.group(1);
      print('方法3匹配到: $id');
      return id;
    }

    // 方法4: 更通用的匹配，查找任何包含 video_id 的URL参数
    final regex4 = RegExp(r'video_id[=:]([a-zA-Z0-9_\-]{15,})');
    final matches4 = regex4.allMatches(html);

    for (final match in matches4) {
      final id = match.group(1);
      print('方法4匹配到: $id');
      return id;
    }

    // 方法5: 解码HTML实体后再次尝试
    try {
      final decodedHtml = html
          .replaceAll(r'\\u002F', '/')
          .replaceAll(r'\\/', '/')
          .replaceAll(r'\"', '"')
          .replaceAll(r"\'", "'");

      final regex5 = RegExp(r'video_id=([a-zA-Z0-9_\-]{15,})');
      final match5 = regex5.firstMatch(decodedHtml);

      if (match5 != null) {
        final id = match5.group(1);
        print('方法5匹配到: $id');
        return id;
      }
    } catch (e) {
      print('HTML解码失败: $e');
    }

    // 调试：输出HTML片段以查看实际内容
    print('HTML中包含video_id的部分:');
    final lines = html.split('\n');
    for (var i = 0; i < lines.length; i++) {
      if (lines[i].contains('video_id')) {
        print('第${i + 1}行: ${lines[i].trim()}');
      }
    }

    return null;
  }

  static String generateVideoUrl(String videoId) {
    return 'https://aweme.snssdk.com/aweme/v1/play/?video_id=$videoId&ratio=default&line=0&h265=1';
  }
}
