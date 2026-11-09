import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:douyin_parser/services/a_bogus.dart';

class AwemeDetail {
  final String awemeId;
  final String? desc;
  final String? playUri;
  final List<String> playUrls;

  AwemeDetail({
    required this.awemeId,
    this.desc,
    this.playUri,
    required this.playUrls,
  });
}

/// Signs and calls douyin web data endpoints without a browser.
///
/// The visitor id ([uifid]) is a device identifier the Argus SDK registers
/// once per device; it is long-lived and travels as both cookie and query
/// parameter. A fresh `ttwid` is minted per session over plain HTTP.
class DouyinApiService {
  static final Dio _dio = Dio();

  /// A long-lived visitor id minted from a real browser session.
  static const String uifid =
      '963b855e412d3196d1c3a882f7f19fd82d0f634397c778689a74b98535e82730a4f3989c9e0abbd48f6171b2f9d78a2e4c7c6db129bbdc9d3136b80e637bad2dee6112dd4ae9ed05d704818aba3519cee3b3d6c5d95a42d0c2d962c96fc1e857321e48c02a520ffaa0b74b363879a721';

  static const String salt = 'A96D855A08C0A9707F8BEF0D9A527E4E';
  static const String userAgent =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36';

  static String? _ttwid;

  /// Fetches one video's detail through the signed web endpoint.
  ///
  /// Returns null when the endpoint refuses the request (risk control,
  /// missing video, etc.) so callers can fall back to the HTML flow.
  static Future<AwemeDetail?> fetchAwemeDetail(String awemeId) async {
    try {
      final ttwid = await _ensureTtwid();
      final svWebId = _generateVerifyFp();
      final msToken = _generateMsToken();

      final pairs = <String, String>{
        'device_platform': 'webapp',
        'aid': '6383',
        'channel': 'channel_pc_web',
        'aweme_id': awemeId,
        'pc_client_type': '1',
        'version_code': '190500',
        'version_name': '19.5.0',
        'cookie_enabled': 'true',
        'screen_width': '1920',
        'screen_height': '1080',
        'browser_language': 'zh-CN',
        'browser_platform': 'Win32',
        'browser_name': 'Chrome',
        'browser_version': '146.0.0.0',
        'browser_online': 'true',
        'engine_name': 'Blink',
        'engine_version': '146.0.0.0',
        'os_name': 'Windows',
        'os_version': '10',
        'cpu_core_num': '8',
        'device_memory': '8',
        'platform': 'PC',
        'msToken': msToken,
      };
      var query = encodeQuery(pairs);
      final bogus = ABogus(userAgent).getValue(query);
      query += '&a_bogus=${_quote(bogus)}';

      final stamp = (DateTime.now().millisecondsSinceEpoch ~/ 1000).toString();
      final qpairs = query
          .split('&')
          .where((part) => part.isNotEmpty)
          .map((part) {
            final name = part.split('=')[0];
            final value = part.substring(part.indexOf('=') + 1);
            return [Uri.decodeComponent(name), Uri.decodeComponent(value)];
          })
          .toList();
      qpairs.add(['verifyFp', svWebId]);
      qpairs.add(['fp', svWebId]);
      qpairs.add(['uifid', uifid]);
      qpairs.add(['timestamp', stamp]);
      final encoded = qpairs
          .map((pair) =>
              '${_websignQuote(pair[0])}=${_websignQuote(pair[1])}')
          .join('&');
      final signature = _md5('${uifid}_${stamp}_${salt}_$encoded');
      final finalQuery = '$encoded&x-secsdk-web-signature=$signature';

      final response = await _dio.get(
        'https://www.douyin.com/aweme/v1/web/aweme/detail/?$finalQuery',
        options: Options(
          headers: {
            'User-Agent': userAgent,
            'Referer': 'https://www.douyin.com/video/$awemeId',
            'Cookie': 'ttwid=$ttwid; s_v_web_id=$svWebId',
            'uifid': uifid,
            'x-secsdk-web-signature': signature,
            'x-secsdk-web-expire': stamp,
          },
          validateStatus: (status) => status != null && status < 500,
        ),
      );
      if (response.statusCode != 200) {
        print('aweme detail HTTP ${response.statusCode}: ${response.data}');
        return null;
      }
      final data = response.data;
      if (data is! Map || data['status_code'] != 0) {
        print('aweme detail status_code: ${data is Map ? data['status_code'] : '?'}');
        return null;
      }
      final detail = data['aweme_detail'];
      if (detail is! Map) return null;
      final video = detail['video'];
      final playAddr = video is Map ? video['play_addr'] : null;
      final urls = <String>[];
      if (playAddr is Map && playAddr['url_list'] is List) {
        for (final url in playAddr['url_list'] as List) {
          if (url is String && url.isNotEmpty) urls.add(url);
        }
      }
      if (urls.isEmpty) return null;
      return AwemeDetail(
        awemeId: awemeId,
        desc: detail['desc'] is String ? detail['desc'] as String : null,
        playUri: playAddr is Map ? playAddr['uri'] as String? : null,
        playUrls: urls,
      );
    } catch (e) {
      print('fetchAwemeDetail failed: $e');
      return null;
    }
  }

  static Future<String> _ensureTtwid() async {
    if (_ttwid != null) return _ttwid!;
    try {
      final response = await _dio.post(
        'https://ttwid.bytedance.com/ttwid/union/register/',
        data: {
          'region': 'cn',
          'aid': 1768,
          'needFid': false,
          'service': 'www.ixigua.com',
          'migrate_info': {'ticket': '', 'source': 'node'},
          'cbUrlProtocol': 'https',
          'union': true,
        },
        options: Options(
          headers: {'Content-Type': 'application/json'},
          validateStatus: (status) => true,
        ),
      );
      final setCookies = response.headers['set-cookie'];
      if (setCookies != null) {
        for (final cookie in setCookies) {
          if (cookie.startsWith('ttwid=')) {
            _ttwid = cookie.split(';').first.substring('ttwid='.length);
            return _ttwid!;
          }
        }
      }
    } catch (e) {
      print('ttwid mint failed: $e');
    }
    return '';
  }

  static String _generateVerifyFp() {
    final milliseconds = DateTime.now().millisecondsSinceEpoch;
    final tail = List<String>.filled(36, '');
    for (final index in [8, 13, 18, 23]) {
      tail[index] = '_';
    }
    tail[14] = '4';
    const alphabet = '0123456789abcdefghijklmnopqrstuvwxyz';
    final random = Random();
    for (var i = 0; i < 36; i++) {
      if (tail[i].isNotEmpty) continue;
      var position = random.nextInt(alphabet.length);
      if (i == 19) {
        position = (3 & position) | 8;
      }
      tail[i] = alphabet[position];
    }
    return 'verify_${_toBase36(milliseconds)}_${tail.join()}';
  }

  static String _generateMsToken() {
    const alphabet =
        'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_';
    final random = Random();
    return String.fromCharCodes(
          List.generate(116, (_) => alphabet.codeUnitAt(random.nextInt(alphabet.length))),
        ) +
        '==';
  }

  static String _toBase36(int value) {
    if (value <= 0) return '0';
    final digits = <String>[];
    var v = value;
    while (v > 0) {
      final remainder = v % 36;
      digits.add(remainder < 10
          ? String.fromCharCode(48 + remainder)
          : String.fromCharCode(97 + remainder - 10));
      v ~/= 36;
    }
    return digits.reversed.join();
  }

  /// Python `urllib.parse.urlencode`: keeps the unreserved set raw.
  static String encodeQuery(Map<String, String> params) {
    return params.entries
        .map((entry) => '${_quote(entry.key)}=${_quote(entry.value)}')
        .join('&');
  }

  /// Python `urllib.parse.quote(value, safe='')`.
  static String _quote(String value) {
    final buffer = StringBuffer();
    for (final code in utf8.encode(value)) {
      if (_isUnreserved(code)) {
        buffer.writeCharCode(code);
      } else {
        buffer.write('%${code.toRadixString(16).toUpperCase().padLeft(2, '0')}');
      }
    }
    return buffer.toString();
  }

  /// Python `urllib.parse.quote(value, safe='*-._')` for the web signature.
  static String _websignQuote(String value) {
    const safe = '*-._';
    final buffer = StringBuffer();
    for (final code in utf8.encode(value)) {
      if (_isUnreserved(code) || safe.contains(String.fromCharCode(code))) {
        buffer.writeCharCode(code);
      } else {
        buffer.write('%${code.toRadixString(16).toUpperCase().padLeft(2, '0')}');
      }
    }
    return buffer.toString();
  }

  static bool _isUnreserved(int code) {
    return (code >= 0x41 && code <= 0x5A) ||
        (code >= 0x61 && code <= 0x7A) ||
        (code >= 0x30 && code <= 0x39) ||
        code == 0x2D ||
        code == 0x2E ||
        code == 0x5F ||
        code == 0x7E;
  }

  static String md5(List<int> input) {
    final digest = _md5Bytes(input);
    return digest.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  }

  static String _md5(String input) => md5(utf8.encode(input));

  static Uint8List _md5Bytes(List<int> message) {
    const s = [
      7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
      5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
      4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
      6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const k = [
      0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a,
      0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
      0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
      0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
      0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
      0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
      0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
      0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
      0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
      0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
      0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    var a0 = 0x67452301;
    var b0 = 0xefcdab89;
    var c0 = 0x98badcfe;
    var d0 = 0x10325476;

    final data = Uint8List.fromList(message);
    final bitLength = data.length * 8;
    final padded = BytesBuilder();
    padded.add(data);
    padded.addByte(0x80);
    while (padded.length % 64 != 56) {
      padded.addByte(0);
    }
    final lengthBytes = ByteData(8)..setUint64(0, bitLength, Endian.little);
    padded.add(lengthBytes.buffer.asUint8List());
    final chunks = padded.toBytes();

    final view = ByteData.sublistView(chunks);
    for (var chunk = 0; chunk < chunks.length; chunk += 64) {
      final m = List<int>.generate(16, (i) => view.getUint32(chunk + i * 4, Endian.little));
      var a = a0, b = b0, c = c0, d = d0;
      for (var i = 0; i < 64; i++) {
        int f, g;
        if (i < 16) {
          f = (b & c) | (~b & d);
          g = i;
        } else if (i < 32) {
          f = (d & b) | (~d & c);
          g = (5 * i + 1) % 16;
        } else if (i < 48) {
          f = b ^ c ^ d;
          g = (3 * i + 5) % 16;
        } else {
          f = c ^ (b | ~d);
          g = (7 * i) % 16;
        }
        f = (f + a + k[i] + m[g]) & 0xFFFFFFFF;
        a = d;
        d = c;
        c = b;
        b = (b + ((f << s[i]) | (f >>> (32 - s[i])))) & 0xFFFFFFFF;
      }
      a0 = (a0 + a) & 0xFFFFFFFF;
      b0 = (b0 + b) & 0xFFFFFFFF;
      c0 = (c0 + c) & 0xFFFFFFFF;
      d0 = (d0 + d) & 0xFFFFFFFF;
    }
    final out = Uint8List(16);
    final outView = ByteData.sublistView(out);
    outView.setUint32(0, a0, Endian.little);
    outView.setUint32(4, b0, Endian.little);
    outView.setUint32(8, c0, Endian.little);
    outView.setUint32(12, d0, Endian.little);
    return out;
  }
}
