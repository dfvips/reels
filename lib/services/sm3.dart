import 'dart:convert';

import 'dart:typed_data';

/// SM3 hash (GB/T 32905-2016), ported from the reference implementation.
///
/// The output is byte-for-byte identical to the published test vectors and to
/// the `gmssl.sm3.sm3_hash` reference the douyin web SDK uses.
const int _mask = 0xFFFFFFFF;

const List<int> _iv = [
  0x7380166F,
  0x4914B2B9,
  0x172442D7,
  0xDA8A0600,
  0xA96F30BC,
  0x163138AA,
  0xE38DEE4D,
  0xB0FB0E4E,
];

const int _t0 = 0x79CC4519;
const int _t1 = 0x7A879D8A;

int _rotl(int value, int bits) {
  bits %= 32;
  return ((value << bits) & _mask) | (value >>> (32 - bits));
}

int _ff(int index, int x, int y, int z) {
  if (index < 16) return x ^ y ^ z;
  return (x & y) | (x & z) | (y & z);
}

int _gg(int index, int x, int y, int z) {
  if (index < 16) return x ^ y ^ z;
  return (x & y) | ((~x & _mask) & z);
}

int _p1(int x) {
  final t = x ^ _rotl(x, 15) ^ _rotl(x, 23);
  return t & _mask;
}

(List<int>, List<int>) _expand(Uint8List block) {
  final w = List<int>.filled(68, 0);
  final view = ByteData.sublistView(block);
  for (var j = 0; j < 16; j++) {
    w[j] = view.getUint32(j * 4);
  }
  for (var j = 16; j < 68; j++) {
    final w3 = w[j - 3];
    final w13 = w[j - 13];
    final x = w[j - 16] ^ w[j - 9] ^ _rotl(w3, 15);
    final p1 = _p1(x);
    w[j] = (p1 ^ _rotl(w13, 7) ^ w[j - 6]) & _mask;
  }
  final w1 = List<int>.generate(64, (j) => w[j] ^ w[j + 4]);
  return (w, w1);
}

List<int> _compress(List<int> state, Uint8List block) {
  final (w, w1) = _expand(block);
  var a = state[0], b = state[1], c = state[2], d = state[3];
  var e = state[4], f = state[5], g = state[6], h = state[7];
  for (var j = 0; j < 64; j++) {
    final t = j < 16 ? _t0 : _t1;
    final ss1 = _rotl((_rotl(a, 12) + e + _rotl(t, j)) & _mask, 7);
    final ss2 = ss1 ^ _rotl(a, 12);
    final tt1 = (_ff(j, a, b, c) + d + ss2 + w1[j]) & _mask;
    final tt2 = (_gg(j, e, f, g) + h + ss1 + w[j]) & _mask;
    d = c;
    c = _rotl(b, 9);
    b = a;
    a = tt1;
    h = g;
    g = _rotl(f, 19);
    f = e;
    e = (tt2 ^ _rotl(tt2, 9) ^ _rotl(tt2, 17)) & _mask;
  }
  final out = [a, b, c, d, e, f, g, h];
  return List.generate(
    8,
    (i) => (state[i] ^ out[i]) & _mask,
  );
}

Uint8List _pad(Uint8List message) {
  final bitLength = message.length * 8;
  final padded = BytesBuilder();
  padded.add(message);
  padded.addByte(0x80);
  while (padded.length % 64 != 56) {
    padded.addByte(0x00);
  }
  final lengthBytes = Uint8List(8);
  final lengthView = ByteData.sublistView(lengthBytes);
  lengthView.setUint64(0, bitLength);
  padded.add(lengthBytes);
  return padded.toBytes();
}

Uint8List sm3Hash(Uint8List message) {
  var state = _iv;
  final padded = _pad(message);
  for (var offset = 0; offset < padded.length; offset += 64) {
    state = _compress(state, Uint8List.sublistView(padded, offset, offset + 64));
  }
  final out = Uint8List(32);
  final view = ByteData.sublistView(out);
  for (var i = 0; i < 8; i++) {
    view.setUint32(i * 4, state[i]);
  }
  return out;
}

List<int> sm3ToArray(Object data) {
  Uint8List raw;
  if (data is String) {
    raw = Uint8List.fromList(utf8.encode(data));
  } else if (data is Uint8List) {
    raw = data;
  } else if (data is List<int>) {
    raw = Uint8List.fromList(data);
  } else {
    throw ArgumentError('unsupported type: ${data.runtimeType}');
  }
  return sm3Hash(raw);
}
