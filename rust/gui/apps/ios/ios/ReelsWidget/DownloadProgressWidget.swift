import ActivityKit
import SwiftUI
import WidgetKit

private let reelsBlue = Color(red: 2 / 255, green: 119 / 255, blue: 249 / 255)
private let reelsPurple = Color(red: 93 / 255, green: 71 / 255, blue: 227 / 255)

@main
struct ReelsWidgetBundle: WidgetBundle {
    var body: some Widget {
        DownloadProgressWidget()
    }
}

struct DownloadProgressWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: DownloadActivityAttributes.self) { context in
            LockScreenDownloadView(state: context.state)
                .activityBackgroundTint(Color(red: 0.055, green: 0.065, blue: 0.085))
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    StatusIcon(state: context.state)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.percentText)
                        .font(.system(size: 20, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                        .monospacedDigit()
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(context.state.title)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Text(context.state.statusText)
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.white.opacity(0.66))
                            .lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 7) {
                        DownloadProgressBar(state: context.state)
                        HStack {
                            Text(context.state.countText)
                            Spacer()
                            Text(context.state.bytesText)
                        }
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.white.opacity(0.62))
                        .monospacedDigit()
                    }
                    .padding(.horizontal, 4)
                    .padding(.bottom, 2)
                }
            } compactLeading: {
                Image(systemName: context.state.systemImage)
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(reelsBlue)
            } compactTrailing: {
                Text(context.state.percentText)
                    .font(.system(size: 13, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
                    .monospacedDigit()
            } minimal: {
                Image(systemName: context.state.systemImage)
                    .font(.system(size: 14, weight: .bold))
                    .foregroundStyle(reelsBlue)
            }
            .keylineTint(reelsBlue)
        }
        .configurationDisplayName("下载进度")
        .description("在锁屏与灵动岛查看后台下载进度。")
    }
}

private struct LockScreenDownloadView: View {
    let state: DownloadActivityAttributes.ContentState

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                StatusIcon(state: state)
                VStack(alignment: .leading, spacing: 3) {
                    Text(state.title)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Text(state.statusText)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.white.opacity(0.65))
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                Text(state.percentText)
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
                    .monospacedDigit()
            }
            DownloadProgressBar(state: state)
            HStack {
                Text(state.countText)
                Spacer()
                Text(state.bytesText)
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(.white.opacity(0.58))
            .monospacedDigit()
        }
        .padding(16)
    }
}

private struct DownloadProgressBar: View {
    let state: DownloadActivityAttributes.ContentState

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.white.opacity(0.14))
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: [reelsBlue, reelsPurple],
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                    )
                    .frame(width: max(proxy.size.width * state.clampedProgress, 4))
            }
        }
        .frame(height: 7)
        .animation(.easeOut(duration: 0.25), value: state.clampedProgress)
    }
}

private struct StatusIcon: View {
    let state: DownloadActivityAttributes.ContentState

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: [reelsBlue.opacity(0.9), reelsPurple.opacity(0.9)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: 36, height: 36)
            Image(systemName: state.systemImage)
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(.white)
        }
    }
}

private extension DownloadActivityAttributes.ContentState {
    var clampedProgress: Double {
        min(max(progress, 0), 1)
    }

    var percentText: String {
        "\(Int((clampedProgress * 100).rounded()))%"
    }

    var countText: String {
        "\(completedCount)/\(totalCount)"
    }

    var bytesText: String {
        if totalBytes > 0 {
            return "\(ByteCountFormatter.string(fromByteCount: downloadedBytes, countStyle: .file)) / "
                + ByteCountFormatter.string(fromByteCount: totalBytes, countStyle: .file)
        }
        return ByteCountFormatter.string(fromByteCount: downloadedBytes, countStyle: .file)
    }

    var statusText: String {
        switch phase {
        case .downloading:
            return totalCount > 1 ? "正在后台下载 \(countText)" : "正在后台下载"
        case .completed:
            return "已保存到相册"
        case .failed:
            return "下载失败"
        case .cancelled:
            return "下载已取消"
        }
    }

    var systemImage: String {
        switch phase {
        case .downloading:
            return "arrow.down"
        case .completed:
            return "checkmark"
        case .failed:
            return "xmark"
        case .cancelled:
            return "minus"
        }
    }
}
