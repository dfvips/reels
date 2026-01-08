import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

class AppSettings extends ChangeNotifier {
  ThemeMode _themeMode = ThemeMode.system;
  String _themeName = '跟随系统';

  ThemeMode get themeMode => _themeMode;
  String get themeName => _themeName;

  AppSettings() {
    _loadTheme();
  }

  Future<void> _loadTheme() async {
    final prefs = await SharedPreferences.getInstance();
    final themeIndex = prefs.getInt('theme_mode') ?? 0;

    switch (themeIndex) {
      case 0:
        _themeMode = ThemeMode.system;
        _themeName = '跟随系统';
        break;
      case 1:
        _themeMode = ThemeMode.light;
        _themeName = '浅色模式';
        break;
      case 2:
        _themeMode = ThemeMode.dark;
        _themeName = '深色模式';
        break;
    }
    notifyListeners();
  }

  Future<void> _saveTheme(int index) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt('theme_mode', index);
  }

  void setThemeMode(ThemeMode mode, String name) {
    _themeMode = mode;
    _themeName = name;

    int index = 0;
    switch (mode) {
      case ThemeMode.system:
        index = 0;
        break;
      case ThemeMode.light:
        index = 1;
        break;
      case ThemeMode.dark:
        index = 2;
        break;
    }

    _saveTheme(index);
    notifyListeners();
  }
}
