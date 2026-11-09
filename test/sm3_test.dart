import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:douyin_parser/services/sm3.dart';

void main() {
  test('SM3 matches the published "abc" test vector', () {
    final digest = sm3Hash(Uint8List.fromList(utf8.encode('abc')));
    expect(
      digest.map((b) => b.toRadixString(16).padLeft(2, '0')).join(),
      '66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0',
    );
  });
}
