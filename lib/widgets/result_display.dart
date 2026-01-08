import 'package:flutter/material.dart';

class ResultDisplay extends StatelessWidget {
  final String result;
  final VoidCallback onCopy;
  final VoidCallback onDownload;

  const ResultDisplay({
    super.key,
    required this.result,
    required this.onCopy,
    required this.onDownload,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      elevation: 2,
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text(
              '解析结果',
              style: TextStyle(
                fontSize: 16,
                fontWeight: FontWeight.bold,
              ),
            ),
            const SizedBox(height: 12),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(8),
              ),
              child: SelectableText(
                result,
                style: const TextStyle(
                  fontFamily: 'monospace',
                ),
              ),
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton.icon(
                    icon: const Icon(Icons.copy, size: 20),
                    label: const Text('复制链接'),
                    onPressed: onCopy,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: ElevatedButton.icon(
                    icon: const Icon(Icons.download, size: 20),
                    label: const Text('下载视频'),
                    onPressed: onDownload,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
