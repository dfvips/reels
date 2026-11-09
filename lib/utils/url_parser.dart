class UrlParser {
  /// 短链（v.douyin.com/xxxx），需要跟随重定向后才能确定视频 ID。
  static final RegExp _shortLinkPattern = RegExp(
    r'https?://v\.douyin\.com/[A-Za-z0-9_-]+',
    caseSensitive: false,
  );

  /// 带视频 ID 的页面链接（video/note/share 以及 jingxuan 等 modal_id 页面）。
  static final RegExp _pageLinkPattern = RegExp(
    r'''https?://(?!v\.)[a-z0-9-]*(?:\.[a-z0-9-]+)*\.douyin\.com/[^\s"'<>]+''',
    caseSensitive: false,
  );

  /// iesdouyin 分享页链接。
  static final RegExp _shareLinkPattern = RegExp(
    r'https?://(?:www\.)?iesdouyin\.com/share/(?:video|note)/(\d+)',
    caseSensitive: false,
  );

  /// 从文本中提取所有抖音视频链接
  static List<String> extractUrls(String text) {
    final urls = <String>[];

    for (final match in _shortLinkPattern.allMatches(text)) {
      final url = match.group(0)!;
      if (!urls.contains(url)) urls.add(url);
    }
    for (final match in _pageLinkPattern.allMatches(text)) {
      final url = match.group(0)!;
      if (!urls.contains(url)) urls.add(url);
    }
    for (final match in _shareLinkPattern.allMatches(text)) {
      final url = match.group(0)!;
      if (!urls.contains(url)) urls.add(url);
    }

    final result = <String>[];
    for (final url in urls) {
      // 短链需要先跟随重定向才能拿到 ID，原样保留交给后续解析。
      if (_shortLinkPattern.hasMatch(url)) {
        if (!result.contains(url)) result.add(url);
        continue;
      }
      final id = extractAwemeId(url);
      if (id == null) continue;
      final normalized = 'https://www.iesdouyin.com/share/video/$id/';
      if (!result.contains(normalized)) result.add(normalized);
    }
    return result;
  }

  /// 从链接中提取数字视频 ID。
  ///
  /// 支持 `jingxuan?modal_id=`、`discover?modal_id=`、`video/{id}`、
  /// `note/{id}` 等页面形式；短链需先跟随重定向。
  static String? extractAwemeId(String url) {
    final modalMatch =
        RegExp(r'modal_id=(\d+)', caseSensitive: false).firstMatch(url);
    if (modalMatch != null) {
      return modalMatch.group(1);
    }
    final videoMatch =
        RegExp(r'/(?:video|note)/(\d+)', caseSensitive: false).firstMatch(url);
    if (videoMatch != null) {
      return videoMatch.group(1);
    }
    return null;
  }

  /// 从单个文本中提取链接（保持向后兼容）
  static String? extractUrl(String text) {
    final urls = extractUrls(text);
    return urls.isNotEmpty ? urls.first : null;
  }

  static String convertDouyinVideoUrl(String url) {
    // 匹配抖音视频链接模式
    final douyinPattern = RegExp(
      r'^https?://(?:www\.)?douyin\.com/video/(\d+)/?$',
    );
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

  /// 从HTML中提取视频标题
  static String? extractVideoTitle(String html) {
    try {
      // 方法1: 从 <title> 标签中提取
      final titleRegex1 = RegExp(
        r'<title[^>]*>(.*?)</title>',
        caseSensitive: false,
      );
      final titleMatch1 = titleRegex1.firstMatch(html);

      if (titleMatch1 != null) {
        final title = titleMatch1.group(1);
        if (title != null && title.isNotEmpty) {
          print('从<title>标签提取到标题: $title');
          return _cleanTitle(title);
        }
      }

      // 方法2: 从 meta 标签中提取 og:title
      final metaRegex = RegExp(
        r'<meta\s+property="og:title"\s+content="([^"]+)"',
        caseSensitive: false,
      );
      final metaMatch = metaRegex.firstMatch(html);

      if (metaMatch != null) {
        final title = metaMatch.group(1);
        if (title != null && title.isNotEmpty) {
          print('从og:title提取到标题: $title');
          return _cleanTitle(title);
        }
      }

      // 方法3: 从抖音特定的数据结构中提取
      final douyinTitleRegex = RegExp(
        r'"desc(?:ription)?"\s*:\s*"([^"]+)"',
        caseSensitive: false,
      );
      final douyinTitleMatch = douyinTitleRegex.firstMatch(html);

      if (douyinTitleMatch != null) {
        final title = douyinTitleMatch.group(1);
        if (title != null && title.isNotEmpty) {
          print('从desc提取到标题: $title');
          return _cleanTitle(title);
        }
      }

      print('未找到视频标题');
      return null;
    } catch (e) {
      print('提取视频标题失败: $e');
      return null;
    }
  }

  /// 清理标题，移除不需要的字符和抖音相关的后缀
  static String _cleanTitle(String title) {
    String cleaned = title.trim();

    // 移除抖音相关后缀
    cleaned = cleaned.replaceAll(RegExp(r'\s*-?\s*抖音.*$'), '');
    cleaned = cleaned.replaceAll(RegExp(r'\s*-?\s*抖音短视频$'), '');
    cleaned = cleaned.replaceAll(RegExp(r'\s*-?\s*记录美好生活$'), '');
    cleaned = cleaned.replaceAll(RegExp(r'\s*#抖音.*$'), '');

    // 移除特殊字符和多余空格
    cleaned = cleaned.replaceAll(RegExp(r'[<>:"/\\|?*]'), ''); // Windows文件名非法字符
    cleaned = cleaned.replaceAll(RegExp(r'\s+'), ' '); // 多个空格合并为一个
    cleaned = cleaned.trim();

    // 限制标题长度
    if (cleaned.length > 100) {
      cleaned = cleaned.substring(0, 100);
    }

    return cleaned.isEmpty ? '抖音视频' : cleaned;
  }

  static String generateVideoUrl(String videoId) {
    return 'https://aweme.snssdk.com/aweme/v1/play/?video_id=$videoId&ratio=default&line=0&h265=1';
  }
}
