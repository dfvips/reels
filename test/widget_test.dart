// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';

import 'package:douyin_parser/main.dart';

void main() {
  testWidgets('App shows HomeScreen and navigates to Settings', (
    WidgetTester tester,
  ) async {
    TestWidgetsFlutterBinding.ensureInitialized();
    const channel = MethodChannel('window_manager');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      switch (call.method) {
        case 'isMaximized':
        case 'isMinimized':
          return false;
        case 'isFullScreen':
          return false;
      }
      return null;
    });

    await tester.pumpWidget(const MyApp());
    await tester.pumpAndSettle();

    expect(find.text('抖音视频解析'), findsOneWidget);

    await tester.tap(find.byTooltip('设置'));
    // 双击识别器会延迟单击约 300ms，推进假时钟让手势竞技场出结果
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();

    expect(find.text('设置'), findsWidgets);
  });
}
