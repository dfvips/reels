import ActivityKit
import Foundation

@available(iOS 16.1, *)
final class DownloadLiveActivityController {
    static let shared = DownloadLiveActivityController()

    private let activityIDKey = "reels.downloadLiveActivityID"
    private let lock = NSLock()
    private var activity: Activity<DownloadActivityAttributes>?
    private var lastPublishedAt = Date.distantPast
    private var lastPublishedProgress = -1.0
    private var requestInFlight = false
    private var pendingFinish: DownloadActivityAttributes.ContentState?
    private var finished = false

    private func withState<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    private init() {
        if let id = UserDefaults.standard.string(forKey: activityIDKey),
           let restored = Activity<DownloadActivityAttributes>.activities.first(where: { $0.id == id }) {
            activity = restored
        }
    }

    func start(totalCount: Int, title: String) {
        withState {
            lastPublishedAt = .distantPast
            lastPublishedProgress = -1.0
            finished = false
        }
        let state = DownloadActivityAttributes.ContentState(
            title: title,
            progress: 0,
            completedCount: 0,
            totalCount: totalCount,
            downloadedBytes: 0,
            totalBytes: 0,
            phase: .downloading
        )
        publish(state, force: true)
    }

    func update(
        title: String,
        progress: Double,
        completedCount: Int,
        totalCount: Int,
        downloadedBytes: Int64,
        totalBytes: Int64
    ) {
        let state = DownloadActivityAttributes.ContentState(
            title: title,
            progress: min(max(progress, 0), 1),
            completedCount: completedCount,
            totalCount: totalCount,
            downloadedBytes: downloadedBytes,
            totalBytes: totalBytes,
            phase: .downloading
        )
        publish(state, force: false)
    }

    func finish(
        phase: DownloadActivityAttributes.Phase,
        title: String,
        progress: Double,
        completedCount: Int,
        totalCount: Int,
        downloadedBytes: Int64,
        totalBytes: Int64
    ) {
        let state = DownloadActivityAttributes.ContentState(
            title: title,
            progress: min(max(progress, 0), 1),
            completedCount: completedCount,
            totalCount: totalCount,
            downloadedBytes: downloadedBytes,
            totalBytes: totalBytes,
            phase: phase
        )
        publish(state, force: true)

        var current: Activity<DownloadActivityAttributes>?
        withState {
            finished = true
            if requestInFlight {
                pendingFinish = state
            } else if let activity {
                current = activity
                self.activity = nil
            }
        }
        guard let current else { return }
        UserDefaults.standard.removeObject(forKey: activityIDKey)
        end(current, state: state)
    }

    private func publish(_ state: DownloadActivityAttributes.ContentState, force: Bool) {
        let now = Date()
        let decision: (active: Activity<DownloadActivityAttributes>?, request: Bool) = withState {
            guard !finished else { return (nil, false) }
            let progressDelta = abs(state.progress - lastPublishedProgress)
            guard force || now.timeIntervalSince(lastPublishedAt) >= 1 ||
                    progressDelta >= 0.02
            else { return (nil, false) }
            lastPublishedAt = now
            lastPublishedProgress = state.progress
            if let current = activity, current.activityState == .active {
                return (current, false)
            }
            if requestInFlight {
                return (nil, false)
            }
            requestInFlight = true
            return (nil, true)
        }

        let content = ActivityContent(state: state, staleDate: nil)
        if let active = decision.active {
            Task {
                await active.update(content)
            }
            return
        }

        guard decision.request else { return }
        Task {
            guard ActivityAuthorizationInfo().areActivitiesEnabled else {
                withState {
                    requestInFlight = false
                    pendingFinish = nil
                }
                return
            }
            do {
                let requested = try Activity<DownloadActivityAttributes>.request(
                    attributes: DownloadActivityAttributes(startedAt: now),
                    content: content,
                    pushType: nil
                )
                let pending: DownloadActivityAttributes.ContentState? = withState {
                    activity = requested
                    requestInFlight = false
                    if let pendingFinish {
                        self.pendingFinish = nil
                        activity = nil
                        return pendingFinish
                    }
                    return nil
                }
                if let pending {
                    UserDefaults.standard.removeObject(forKey: activityIDKey)
                    end(requested, state: pending)
                } else {
                    UserDefaults.standard.set(requested.id, forKey: activityIDKey)
                }
            } catch {
                withState {
                    requestInFlight = false
                    pendingFinish = nil
                }
                NSLog("reels: live activity start failed: %@", error.localizedDescription)
            }
        }
    }

    private func end(
        _ current: Activity<DownloadActivityAttributes>,
        state: DownloadActivityAttributes.ContentState
    ) {
        let delay: TimeInterval = state.phase == .completed ? 4 : 8
        Task {
            let content = ActivityContent(state: state, staleDate: nil)
            await current.end(
                content,
                dismissalPolicy: .after(Date().addingTimeInterval(delay))
            )
        }
    }
}
