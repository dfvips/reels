import 'package:flutter_test/flutter_test.dart';
import 'package:douyin_parser/utils/url_parser.dart';

void main() {
  group('extractUrls', () {
    test('jingxuan modal_id link', () {
      final urls = UrlParser.extractUrls(
        'https://www.douyin.com/jingxuan?modal_id=7701193143558901018',
      );
      expect(urls, [
        'https://www.iesdouyin.com/share/video/7701193143558901018/',
      ]);
    });

    test('discover modal_id link', () {
      final urls = UrlParser.extractUrls(
        'https://www.douyin.com/discover?modal_id=7301234567890123456',
      );
      expect(urls, [
        'https://www.iesdouyin.com/share/video/7301234567890123456/',
      ]);
    });

    test('video page link', () {
      final urls = UrlParser.extractUrls(
        'https://www.douyin.com/video/7301234567890123456',
      );
      expect(urls, [
        'https://www.iesdouyin.com/share/video/7301234567890123456/',
      ]);
    });

    test('note page link', () {
      final urls = UrlParser.extractUrls(
        'https://www.douyin.com/note/7301234567890123456',
      );
      expect(urls, [
        'https://www.iesdouyin.com/share/video/7301234567890123456/',
      ]);
    });

    test('short share text with jingxuan link inside', () {
      final urls = UrlParser.extractUrls(
        '看看这个 https://www.douyin.com/jingxuan?modal_id=7701193143558901018 '
        '挺有意思的',
      );
      expect(urls.length, 1);
      expect(urls.first, contains('7701193143558901018'));
    });

    test('deduplicates the same video id from different pages', () {
      final urls = UrlParser.extractUrls(
        'https://www.douyin.com/video/7701193143558901018 '
        'https://www.douyin.com/jingxuan?modal_id=7701193143558901018',
      );
      expect(urls.length, 1);
    });

    test('keeps short links unresolved', () {
      final urls = UrlParser.extractUrls('https://v.douyin.com/iAbCdEf/');
      expect(urls, ['https://v.douyin.com/iAbCdEf']);
    });
  });

  group('extractAwemeId', () {
    test('modal_id', () {
      expect(
        UrlParser.extractAwemeId(
          'https://www.douyin.com/jingxuan?modal_id=7701193143558901018',
        ),
        '7701193143558901018',
      );
    });

    test('video path', () {
      expect(
        UrlParser.extractAwemeId('https://www.douyin.com/video/730123'),
        '730123',
      );
    });

    test('no id', () {
      expect(UrlParser.extractAwemeId('https://v.douyin.com/iAbCdEf/'), isNull);
    });
  });
}
