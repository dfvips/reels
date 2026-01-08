import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:bot_toast/bot_toast.dart';

class ClipboardUtils {
  static Future<void> copyToClipboard(String text) async {
    if (text.isEmpty) return;

    await Clipboard.setData(ClipboardData(text: text));

    BotToast.showText(
      text: '已复制到剪贴板',
      duration: const Duration(seconds: 2),
      contentColor: Colors.black54,
      textStyle: const TextStyle(color: Colors.white, fontSize: 16.0),
    );
  }
}
