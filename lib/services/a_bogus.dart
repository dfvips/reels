import 'dart:math';
import 'dart:typed_data';

import 'package:douyin_parser/services/sm3.dart';

const Map<String, String> _alphabets = {
  's3': 'ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe',
  's4': 'Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe',
};

/// A-Bogus, the signature douyin web puts on every data endpoint.
///
/// Dart port of the algorithm reversed from douyin's `bdms.js`
/// v1.0.1.19-fix.01. The reference implementation carries three random
/// "noise" layers that never reach the checksum; this port draws them from
/// [Random] at call time, exactly as the real SDK does.
class ABogus {
  ABogus(this.userAgent, {String? browserInfo, Random? random})
      : browserInfo = browserInfo ?? defaultBrowserInfo,
        _random = random ?? Random();

  final String userAgent;
  final String browserInfo;
  final Random _random;

  static const String salt = 'dhzx';
  static const List<int> headerMagic = [3, 82];
  static const List<int> sdkVersion = [1, 0, 1, 0];
  static const int payloadKey = 0xD3;
  static const int fortnightEpochMs = 1721836800000;
  static const int pageId = 6241;
  static const int aid = 6383;
  static const List<int> noiseMasks = [0x91, 0x42, 0x2C];
  static const List<int> dataMasks = [0x6E, 0xBD, 0xD3];
  static const int envFlags = 1;
  static const int detectFlags = 14;
  static const int nrFlags = 0x21;
  static const int tripwireLocked = 3;
  static const int callBucket = 6;
  static const String defaultBrowserInfo =
      '1920|947|1920|1032|1920|1032|1920|1080|Win32';

  static const List<String> _fieldOrder = [
    'L34', 'L44', 'L56', 'L61', 'L73', 'L29', 'L70', 'L45', 'L35', 'L49',
    'L38', 'L66', 'L51', 'L68', 'L28', 'L48', 'L64', 'L47', 'L30', 'L71',
    'L26', 'L55', 'L31', 'L69', 'L59', 'L40', 'L62', 'L63', 'L27', 'L72',
    'L41', 'L74', 'L57', 'L52', 'L42', 'L39', 'L33', 'L67', 'L53', 'L43',
    'L65', 'L46', 'L36', 'L24', 'L60', 'L32', 'L79', 'L80', 'L84', 'L85',
  ];

  static const List<int> _envTag = [0, 0, 0, 0];

  /// The five bytes `fn149`'s header carries after masking.
  static const int _tripwireSet = 0xB2;
  static const int _tripwireFree = 0x4D;

  static const Map<String, int> _headerNoiseBands = {
    'chrome': 0,
    'firefox': 40,
    'safari': 81,
    'edge': 125,
    'huawei': 170,
    'other': 210,
  };

  /// The signature for one request.
  ///
  /// [query] is the query string as it will be sent, without `a_bogus`
  /// itself. [body] is the request body for a POST and empty otherwise.
  String getValue(String query, {String body = '', int? nowMs}) {
    final now = nowMs ?? DateTime.now().millisecondsSinceEpoch;
    if (now < fortnightEpochMs) {
      throw ArgumentError('clock is before the a_bogus epoch: $now');
    }
    final fields = _fields(query, body, now);

    final version = <int>[
      ..._maskPair([sdkVersion[0], sdkVersion[1]]),
      ..._maskPair(
        [sdkVersion[2], sdkVersion[3]],
        low: _probeNoise(),
        high: _tripwireNoise(),
      ),
    ];

    var checksum = 0;
    for (final byte in version) {
      checksum ^= byte;
    }
    for (final name in _fieldOrder) {
      checksum ^= fields[name]!;
    }

    final bodyBytes = BytesBuilder();
    for (final name in _fieldOrder) {
      bodyBytes.addByte(fields[name]!);
    }
    bodyBytes.add(jsBytes(browserInfo));
    bodyBytes.add(jsBytes('${(now + 3) & 0xFF},'));
    bodyBytes.addByte(checksum & 0xFF);

    final header = _maskPair(
      [headerMagic[0], headerMagic[1]],
      high: _headerNoise(userAgent),
    );
    final frame = _expandNoise(bodyBytes.toBytes());
    final sealed = rc4(
      Uint8List.fromList([payloadKey]),
      Uint8List.fromList([...version, ...frame]),
    );
    return encodeBase64(Uint8List.fromList([...header, ...sealed]), 's4');
  }

  Map<String, int> _fields(String query, String body, int nowMs) {
    final digests = <String, List<int>>{
      'query': _digestOf(query),
      'body': _digestOf(body),
      'user_agent': userAgentDigest(userAgent),
    };

    final ink = nowMs - 1;
    final fortnights =
        (nowMs - fortnightEpochMs) ~/ (1000 * 60 * 60 * 24 * 14);
    final infoBytes = jsBytes(browserInfo);
    final tailBytes = jsBytes('${(nowMs + 3) & 0xFF},');

    final fields = <String, int>{
      'L24': 41,
      'L26': fortnights,
      'L27': callBucket,
      'L28': 3,
      'L35': envFlags & 0xFF,
      'L36': (envFlags ~/ 256) & 0xFF,
      'L38': nrFlags & 0xFF,
      'L39': (nrFlags >> 8) & 0xFF,
      'L66': tripwireLocked,
      'L79': infoBytes.length & 0xFF,
      'L80': (infoBytes.length >> 8) & 0xFF,
      'L84': tailBytes.length & 0xFF,
      'L85': (tailBytes.length >> 8) & 0xFF,
    };
    final nowLe = _leBytes(nowMs, 6);
    for (var i = 0; i < 6; i++) {
      fields['L${29 + i}'] = nowLe[i];
    }
    final detectLe = _leBytes(detectFlags, 4);
    for (var i = 0; i < 4; i++) {
      fields['L${44 + i}'] = detectLe[i];
    }
    for (var i = 0; i < 4; i++) {
      fields['L${40 + i}'] = _envTag[i];
    }
    final inkLe = _leBytes(ink, 6);
    for (var i = 0; i < 6; i++) {
      fields['L${60 + i}'] = inkLe[i];
    }
    final pageLe = _leBytes(pageId, 4);
    for (var i = 0; i < 4; i++) {
      fields['L${67 + i}'] = pageLe[i];
    }
    final aidLe = _leBytes(aid, 4);
    for (var i = 0; i < 4; i++) {
      fields['L${71 + i}'] = aidLe[i];
    }

    const chains = <String, (List<String>, List<int>, List<int>)>{
      'query': (['L48', 'L49', 'L51'], [9, 18], [3, 11, 12]),
      'body': (['L52', 'L53', 'L55'], [10, 19], [4, 8, 9]),
      'user_agent': (['L56', 'L57', 'L59'], [11, 21], [5, 12, 13]),
    };
    chains.forEach((name, chain) {
      final (slots, indices, canarySpec) = chain;
      final digest = digests[name]!;
      final written = _chainBytes(digest, indices, canarySpec);
      for (var i = 0; i < slots.length; i++) {
        fields[slots[i]] = written[i];
      }
    });
    return fields;
  }

  List<int> _chainBytes(
    List<int> digest,
    List<int> indices,
    List<int> canarySpec,
  ) {
    return [
      digest[indices[0]],
      digest[indices[1]],
      _canary(digest, canarySpec[0], canarySpec[1], canarySpec[2]),
    ];
  }

  int _canary(List<int> digest, int offset, int sentinel, int fallback) {
    for (var i = offset; i < digest.length; i++) {
      if (digest[i] != sentinel) return digest[i];
    }
    return fallback;
  }

  List<int> _maskPair(List<int> pair, {int? low, int? high}) {
    final noise = _random.nextInt(65535);
    final lowValue = (low ?? noise) & 0xFF;
    final highValue = (high ?? (noise >> 8)) & 0xFF;
    return [
      (lowValue & 0xAA) | (pair[0] & 0x55),
      (lowValue & 0x55) | (pair[0] & 0xAA),
      (highValue & 0xAA) | (pair[1] & 0x55),
      (highValue & 0x55) | (pair[1] & 0xAA),
    ];
  }

  int _headerNoise(String ua) {
    final base = _headerNoiseBands[_familyOf(ua)]!;
    return (base + _random.nextInt(40)) & 0xFF;
  }

  int _probeNoise() {
    final value = _random.nextInt(240);
    return value > 109 ? value + value % 2 + 1 : value;
  }

  int _tripwireNoise() {
    return (_random.nextInt(255) & _tripwireFree) | _tripwireSet;
  }

  Uint8List _expandNoise(Uint8List body) {
    final out = BytesBuilder();
    for (var offset = 0; offset < body.length; offset += 3) {
      final remaining = body.length - offset;
      if (remaining < 3) {
        out.addByte(body[offset]);
        if (remaining > 1 && body[offset + 1] != 0) {
          out.addByte(body[offset + 1]);
        }
        continue;
      }
      final noise = _random.nextInt(1000) & 0xFF;
      for (var i = 0; i < 3; i++) {
        out.addByte(
          (noise & noiseMasks[i]) | (body[offset + i] & dataMasks[i]),
        );
      }
      out.addByte(
        (body[offset] & noiseMasks[0]) |
            (body[offset + 1] & noiseMasks[1]) |
            (body[offset + 2] & noiseMasks[2]),
      );
    }
    return out.toBytes();
  }

  static String _familyOf(String userAgent) {
    final ua = userAgent.toLowerCase();
    for (final name in ['edg', 'huawei', 'firefox', 'chrome', 'safari']) {
      if (ua.contains(name)) {
        if (name == 'edg') return 'edge';
        return name;
      }
    }
    return 'other';
  }

  static List<int> _leBytes(int value, int count) {
    return [for (var i = 0; i < count; i++) (value >> (8 * i)) & 0xFF];
  }

  static List<int> _digestOf(String text) {
    return sm3ToArray(sm3ToArray(text + salt));
  }

  static List<int> userAgentDigest(String userAgent) {
    final key = Uint8List.fromList(
      [envFlags ~/ 256, envFlags % 256, detectFlags % 256],
    );
    final raw = jsBytes(userAgent.trim());
    final masked = Uint8List.fromList(
      [for (final byte in raw) byte & 0xFF],
    );
    final sealed = rc4(key, masked);
    return sm3ToArray(encodeBase64(sealed, 's3'));
  }
}

/// The SDK's RC4, which is not RC4: the S-box starts descending and the key
/// schedule multiplies where the textbook one only adds.
Uint8List rc4(Uint8List key, Uint8List data) {
  final box = List<int>.generate(256, (i) => i);
  var j = 0;
  for (var i = 0; i < 256; i++) {
    box[255 - i] = i;
  }
  for (var i = 0; i < 256; i++) {
    j = (j * box[i] + j + key[i % key.length]) % 256;
    final tmp = box[i];
    box[i] = box[j];
    box[j] = tmp;
  }
  final out = Uint8List(data.length);
  var i = 0;
  j = 0;
  for (var index = 0; index < data.length; index++) {
    i = (i + 1) % 256;
    j = (j + box[i]) % 256;
    final tmp = box[i];
    box[i] = box[j];
    box[j] = tmp;
    out[index] = data[index] ^ box[(box[i] + box[j]) % 256];
  }
  return out;
}

String encodeBase64(Uint8List data, String alphabet) {
  final table = _alphabets[alphabet]!;
  final out = StringBuffer();
  for (var offset = 0; offset < data.length; offset += 3) {
    final remaining = data.length - offset;
    var block = 0;
    for (var i = 0; i < 3; i++) {
      block <<= 8;
      if (i < remaining) block |= data[offset + i];
    }
    final digits = [
      (block >> 18) & 0x3F,
      (block >> 12) & 0x3F,
      (block >> 6) & 0x3F,
      block & 0x3F,
    ];
    out.write(table[digits[0]]);
    out.write(table[digits[1]]);
    if (remaining >= 2) out.write(table[digits[2]]);
    if (remaining >= 3) out.write(table[digits[3]]);
  }
  final padLength = (4 - out.length % 4) % 4;
  out.write('=' * padLength);
  return out.toString();
}

/// A JavaScript string as the SDK turns it into bytes: one byte per UTF-16
/// code unit below U+0100 and two big-endian bytes above it.
Uint8List jsBytes(String text) {
  final out = BytesBuilder();
  for (var i = 0; i < text.length; i++) {
    final code = text.codeUnitAt(i);
    if (code & 0xFF00 != 0) {
      out.addByte(code >> 8);
    }
    out.addByte(code & 0xFF);
  }
  return out.toBytes();
}
