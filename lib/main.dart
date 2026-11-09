import 'package:bot_toast/bot_toast.dart';
import 'package:douyin_parser/services/download_service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart'; // 添加本地化
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/screens/home_screen.dart';
import 'package:douyin_parser/screens/settings_screen.dart';
import 'package:douyin_parser/utils/theme_manager.dart';
import 'package:permission_handler/permission_handler.dart'; // 添加权限处理
import 'dart:io'; // 添加平台判断
import 'package:window_manager/window_manager.dart';

final GlobalKey<NavigatorState> navigatorKey = GlobalKey<NavigatorState>();

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await ThemeManager.init();
  await DownloadService.initializeDefaultSettings();

  // 桌面端窗口初始化：无边框标题栏，默认居中显示
  if (Platform.isWindows || Platform.isMacOS) {
    await windowManager.ensureInitialized();
    const windowOptions = WindowOptions(
      size: Size(1024, 720),
      minimumSize: Size(460, 600),
      center: true,
      title: '抖音视频解析',
      titleBarStyle: TitleBarStyle.hidden,
    );
    await windowManager.waitUntilReadyToShow(windowOptions, () async {
      await windowManager.show();
      await windowManager.focus();
    });
  }

  // 在应用启动时请求网络权限（仅移动端）
  if (Platform.isAndroid || Platform.isIOS) {
    await _requestNetworkPermission();
  }

  runApp(const MyApp());
}

// 添加网络权限请求函数
Future<void> _requestNetworkPermission() async {
  try {
    print('启动时检查网络权限...');

    // 请求网络权限
    final status = await Permission.accessMediaLocation.status;
    if (status.isDenied) {
      await Permission.accessMediaLocation.request();
    }

    // 对于Android，还需要检查网络状态权限
    if (Platform.isAndroid) {
      final networkStatus = await Permission.accessNotificationPolicy.status;
      if (networkStatus.isDenied) {
        await Permission.accessNotificationPolicy.request();
      }
    }

    print('网络权限检查完成');
  } catch (e) {
    print('网络权限请求失败: $e');
  }
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider(
      create: (context) => AppSettings(),
      child: Consumer<AppSettings>(
        builder: (context, settings, child) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            navigatorKey: navigatorKey,
            title: '抖音视频解析',

            // 添加本地化委托
            localizationsDelegates: const [
              GlobalMaterialLocalizations.delegate, // 提供 Material 组件本地化字符串
              GlobalWidgetsLocalizations.delegate, // 提供基础 Widget 本地化
              GlobalCupertinoLocalizations.delegate, // 提供 Cupertino 组件本地化
            ],

            // 支持的语言环境
            supportedLocales: const [
              Locale('zh', 'CN'), // 中文简体，中国
              Locale('zh', 'TW'), // 中文繁体，台湾
              Locale('zh', 'HK'), // 中文繁体，香港
              Locale('en', 'US'), // 英文，美国
            ],

            // 设置默认语言环境为简体中文
            locale: const Locale('zh', 'CN'),

            // 当不支持的语言环境被请求时的回退语言环境
            localeResolutionCallback: (locale, supportedLocales) {
              // 如果当前语言环境为null，返回默认的简体中文
              if (locale == null) {
                return const Locale('zh', 'CN');
              }

              // 检查是否支持精确的语言环境
              for (var supportedLocale in supportedLocales) {
                if (supportedLocale.languageCode == locale.languageCode &&
                    supportedLocale.countryCode == locale.countryCode) {
                  return supportedLocale;
                }
              }

              // 检查是否支持相同的语言代码（不同地区）
              for (var supportedLocale in supportedLocales) {
                if (supportedLocale.languageCode == locale.languageCode) {
                  return supportedLocale;
                }
              }

              // 默认返回简体中文
              return const Locale('zh', 'CN');
            },

            theme: ThemeData.light().copyWith(
              primaryColor: Color(0xFF3586F9),
              colorScheme: ColorScheme.fromSeed(
                seedColor: Color(0xFF3586F9),
                primary: Color(0xFF3586F9),
                brightness: Brightness.light,
              ),
              appBarTheme: AppBarTheme(
                backgroundColor: Colors.white,
                surfaceTintColor: Colors.transparent,
                titleTextStyle: TextStyle(
                  color: Color(0xFF3586F9),
                  fontSize: 20,
                  fontWeight: FontWeight.bold,
                ),
                iconTheme: IconThemeData(color: Color(0xFF3586F9)),
                centerTitle: false,
              ),
              scaffoldBackgroundColor: Colors.white,
            ),

            darkTheme: ThemeData.dark().copyWith(
              primaryColor: Color(0xFF3586F9),
              colorScheme: ColorScheme.fromSeed(
                seedColor: Color(0xFF3586F9),
                primary: Color(0xFF3586F9),
                brightness: Brightness.dark,
              ),
              appBarTheme: AppBarTheme(
                backgroundColor: Color(0xFF141414),
                surfaceTintColor: Colors.transparent,
                titleTextStyle: TextStyle(
                  color: Color(0xFF3586F9),
                  fontSize: 20,
                  fontWeight: FontWeight.bold,
                ),
                iconTheme: IconThemeData(color: Color(0xFF3586F9)),
                centerTitle: false,
              ),
              scaffoldBackgroundColor: Color(0xFF141414),
            ),

            themeMode: settings.themeMode,
            initialRoute: '/',
            routes: {
              '/': (context) => const HomeScreen(),
              '/settings': (context) => const SettingsScreen(),
            },
            builder: BotToastInit(),
            navigatorObservers: [BotToastNavigatorObserver()],
          );
        },
      ),
    );
  }
}
