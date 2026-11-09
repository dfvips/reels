import 'package:flutter_test/flutter_test.dart';
import 'package:douyin_parser/services/douyin_api_service.dart';

void main() {
  test('live: signed detail endpoint returns the play url', () async {
    final detail = await DouyinApiService.fetchAwemeDetail(
      '7701193143558901018',
    );
    expect(detail, isNotNull, reason: 'the signed request must be accepted');
    expect(detail!.playUrls, isNotEmpty);
    expect(detail.playUrls.first, contains('douyinvod.com'));
    print('desc: ${detail.desc}');
    print('uri: ${detail.playUri}');
    print('url: ${detail.playUrls.first}');
  }, timeout: const Timeout(Duration(seconds: 60)));
}
