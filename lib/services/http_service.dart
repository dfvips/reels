import 'package:dio/dio.dart';

class HttpService {
  static final Dio _dio = Dio();

  static Future<String> fetchHtml(String url, {bool showTip = true}) async {
    try {
      String currentUrl = url;
      int redirectCount = 0;
      const maxRedirects = 5;

      // 追踪重定向链
      while (redirectCount < maxRedirects) {
        final response = await _dio.get(
          currentUrl,
          options: Options(
            headers: {
              'Accept':
                  'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7',
              'Accept-Encoding': 'deflate',
              'Accept-Language': 'zh-CN,zh;q=0.9',
              'Priority': 'u=0, i',
              'Sec-Fetch-Dest': 'document',
              'Sec-Fetch-Mode': 'navigate',
              'Sec-Fetch-Site': 'none',
              'Sec-Fetch-User': '?1',
              'Upgrade-Insecure-Requests': '1',
              'User-Agent':
                  'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1',
            },
            followRedirects: false, // 关键：设置为false手动处理
            maxRedirects: 0,
            validateStatus: (status) => true, // 接受所有状态码
          ),
        );

        if (showTip == false) {
          return '';
        }

        // 检查是否是重定向状态码
        if (response.statusCode == 301 ||
            response.statusCode == 302 ||
            response.statusCode == 303 ||
            response.statusCode == 307 ||
            response.statusCode == 308) {
          // 获取重定向地址
          final location = response.headers['location']?.first;
          if (location != null && location.isNotEmpty) {
            redirectCount++;

            // 处理相对路径或绝对路径
            Uri redirectUri;
            if (location.startsWith('http')) {
              redirectUri = Uri.parse(location);
            } else {
              redirectUri = Uri.parse(
                Uri.parse(currentUrl).resolve(location).toString(),
              );
            }

            print(
              '[$redirectCount] 重定向 ${response.statusCode} -> ${redirectUri.toString()}',
            );
            currentUrl = redirectUri.toString();
            return fetchHtml(currentUrl);
          } else {
            throw Exception('重定向响应中没有Location头');
          }
        }

        // 如果不是重定向，检查是否是成功响应
        if (response.statusCode == 200) {
          return response.data.toString();
        } else {
          throw Exception('HTTP请求失败: ${response.statusCode}');
        }
      }

      throw Exception('达到最大重定向次数: $maxRedirects');
    } catch (e) {
      if (showTip) {
        // throw Exception('网络请求错误: $e');
        throw Exception('网络请求异常，请在系统设置中允许本应用使用网络');
      } else {
        return '';
      }
    }
  }

  static Future<String?> getRedirectUrl(String url) async {
    try {
      final response = await _dio.head(
        url,
        options: Options(
          headers: {
            'User-Agent':
                'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1',
            'Accept': '*/*',
            'Accept-Language': 'zh-CN,zh;q=0.9',
          },
          followRedirects: false, // 不自动跳转，手动处理302
          validateStatus: (status) => status! < 400,
        ),
      );

      // 检查是否有重定向
      if (response.statusCode == 301 || response.statusCode == 302) {
        final location = response.headers['location']?.first;
        if (location != null && location.isNotEmpty) {
          return location;
        }
      }

      return null;
    } catch (e) {
      return null;
    }
  }

  static Future<String?> getFinalVideoUrl(String videoId) async {
    try {
      final url =
          'https://aweme.snssdk.com/aweme/v1/play/?video_id=$videoId&ratio=default&line=0&h265=1';

      // 先尝试HEAD请求获取重定向URL
      final redirectUrl = await getRedirectUrl(url);
      if (redirectUrl != null && redirectUrl.isNotEmpty) {
        return redirectUrl;
      }

      // 如果HEAD请求失败，尝试GET请求
      final response = await _dio.get(
        url,
        options: Options(
          headers: {
            'User-Agent':
                'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1',
          },
          followRedirects: false,
          validateStatus: (status) => true, // 接受所有状态码
        ),
      );

      if (response.statusCode == 302 || response.statusCode == 301) {
        final location = response.headers['location']?.first;
        return location;
      }

      return url; // 返回原始URL
    } catch (e) {
      print('获取视频链接失败: $e');
      return null;
    }
  }
}
