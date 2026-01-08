import 'package:shared_preferences/shared_preferences.dart';

class ThemeManager {
  static Future<void> init() async {
    // 初始化主题设置
    final prefs = await SharedPreferences.getInstance();
    if (!prefs.containsKey('theme_mode')) {
      await prefs.setInt('theme_mode', 0); // 默认跟随系统
    }
  }
}
