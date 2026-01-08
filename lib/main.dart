import 'package:bot_toast/bot_toast.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:douyin_parser/models/app_settings.dart';
import 'package:douyin_parser/screens/home_screen.dart';
import 'package:douyin_parser/screens/settings_screen.dart';
import 'package:douyin_parser/utils/theme_manager.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await ThemeManager.init();
  runApp(const MyApp());
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
            title: '抖音视频解析工具',
            theme: ThemeData.light().copyWith(
              primaryColor: Colors.red,
              colorScheme: ColorScheme.fromSeed(
                seedColor: Colors.red,
                brightness: Brightness.light,
              ),
            ),
            darkTheme: ThemeData.dark().copyWith(
              primaryColor: Colors.red[800],
              colorScheme: ColorScheme.fromSeed(
                seedColor: Colors.red[800]!,
                brightness: Brightness.dark,
              ),
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
