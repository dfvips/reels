import ActivityKit
import Foundation

struct DownloadActivityAttributes: ActivityAttributes {
    enum Phase: String, Codable, Hashable {
        case downloading
        case completed
        case failed
        case cancelled
    }

    struct ContentState: Codable, Hashable {
        var title: String
        var progress: Double
        var completedCount: Int
        var totalCount: Int
        var downloadedBytes: Int64
        var totalBytes: Int64
        var phase: Phase
    }

    var startedAt: Date
}
