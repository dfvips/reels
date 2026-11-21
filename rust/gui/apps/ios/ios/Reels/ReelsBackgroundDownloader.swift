import Foundation
import Photos
import UIKit

private struct BackgroundDownloadPart: Codable {
    var index: Int
    var start: Int64
    var end: Int64
    var cachePath: String
    var downloaded: Int64 = 0
    var finished = false

    var expected: Int64 {
        max(end - start + 1, 0)
    }
}

private struct BackgroundDownloadItem: Codable {
    var index: Int
    var title: String
    var url: String
    var fileExtension: String
    var liveVideoURL: String?
    var destinationPath: String
    var mainCachePath: String
    var liveCachePath: String?
    var expectedMainSize: Int64 = 0
    var mainSegments: [BackgroundDownloadPart]? = nil
    var mainTotal: Int64 = 0
    var liveTotal: Int64 = 0
    var mainDownloaded: Int64 = 0
    var liveDownloaded: Int64 = 0
    var mainStartedReported = false
    var liveStartedReported = false
    var mainFinished = false
    var liveFinished = false
    var finalized = false
    var failed = false
    var saved = false
    var error: String?

    var hasLiveVideo: Bool {
        liveVideoURL != nil && liveCachePath != nil
    }

    var downloadedBytes: Int64 {
        mainDownloaded + (hasLiveVideo ? liveDownloaded : 0)
    }

    var totalBytes: Int64 {
        mainTotal + (hasLiveVideo ? liveTotal : 0)
    }

    var isTerminal: Bool {
        finalized || failed
    }

}

private struct BackgroundDownloadBatch: Codable {
    var id: String
    var directory: String
    var cacheDirectory: String
    var items: [BackgroundDownloadItem]
    var startedAt: Date
    var cancelled = false
    var finalized = false
    var permissionDenied = false
    var permissionReminderSent = false
}

private enum DownloadTaskKind: String {
    case main
    case live
}

private struct DownloadTaskDescriptor {
    let batchID: String
    let index: Int
    let kind: DownloadTaskKind
    let segment: Int?

    init?(description: String?) {
        let parts = description?.split(separator: "|", omittingEmptySubsequences: false) ?? []
        guard (parts.count == 3 || parts.count == 4),
              let index = Int(parts[1]),
              let kind = DownloadTaskKind(rawValue: String(parts[2]))
        else { return nil }
        batchID = String(parts[0])
        self.index = index
        self.kind = kind
        segment = parts.count == 4 ? Int(parts[3]) : nil
    }

    static func encode(
        batchID: String,
        index: Int,
        kind: DownloadTaskKind,
        segment: Int? = nil
    ) -> String {
        if let segment {
            return "\(batchID)|\(index)|\(kind.rawValue)|\(segment)"
        }
        return "\(batchID)|\(index)|\(kind.rawValue)"
    }
}

@available(iOS 16.1, *)
final class ReelsBackgroundDownloader: NSObject {
    static let shared = ReelsBackgroundDownloader()

    private static let sessionIdentifier = "com.dreamfly.reels.background-downloads"
    private static let stateKey = "reels.backgroundDownloadBatch.v1"
    private static let userAgent =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
        "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
    private static let segmentedDownloadThreshold: Int64 = 50 * 1024 * 1024
    private static let segmentCount = 6

    private let queue = DispatchQueue(label: "com.dreamfly.reels.background-download")
    private var session: URLSession!
    private var batch: BackgroundDownloadBatch?
    private var backgroundCompletionHandler: (() -> Void)?
    private var lastReportedBytes: [String: Int64] = [:]
    private var pendingFinalizations = 0

    private override init() {
        super.init()
        let configuration = URLSessionConfiguration.background(
            withIdentifier: Self.sessionIdentifier
        )
        configuration.sessionSendsLaunchEvents = true
        configuration.isDiscretionary = false
        configuration.waitsForConnectivity = true
        configuration.httpMaximumConnectionsPerHost = 6
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 24 * 60 * 60

        let delegateQueue = OperationQueue()
        delegateQueue.name = "com.dreamfly.reels.background-download.delegate"
        delegateQueue.maxConcurrentOperationCount = 1
        session = URLSession(
            configuration: configuration,
            delegate: self,
            delegateQueue: delegateQueue
        )
        restoreState()
    }

    func activate() {
        queue.async { [weak self] in
            guard let self else { return }
            if self.batch == nil {
                self.cleanStaleTasks()
            } else {
                self.finishIfReady()
            }
        }
    }

    func handleEventsForBackgroundURLSession(completionHandler: @escaping () -> Void) {
        queue.async { [weak self] in
            self?.backgroundCompletionHandler = completionHandler
            self?.completeBackgroundEventsIfReady()
        }
    }

    func start(
        batchID: String,
        urls: [String],
        titles: [String],
        extensions: [String?],
        liveVideos: [String?],
        sizes: [UInt64],
        directory: String,
        cacheDirectory: String
    ) -> Bool {
        guard !urls.isEmpty,
              urls.count == titles.count,
              urls.count == extensions.count,
              urls.count == liveVideos.count,
              urls.count == sizes.count
        else { return false }

        var accepted = false
        queue.sync {
            if let existing = batch, !existing.finalized {
                cancelTasks(for: existing.id)
                batch = nil
            }
            PhotoLibraryPermission.resetReminderForNewAttempt()
            let fileManager = FileManager.default
            do {
                try fileManager.createDirectory(
                    atPath: directory,
                    withIntermediateDirectories: true
                )
                try fileManager.createDirectory(
                    atPath: cacheDirectory,
                    withIntermediateDirectories: true
                )
            } catch {
                NSLog("reels: create download directories failed: %@", error.localizedDescription)
                return
            }

            var plannedNames = Set<String>()
            var items: [BackgroundDownloadItem] = []
            for index in urls.indices {
                let rawExtension = extensions[index]?.trimmingCharacters(in: .whitespacesAndNewlines)
                let fileExtension = (rawExtension?.isEmpty == false ? rawExtension! : "mp4")
                    .lowercased()
                let base = Self.downloadBaseName(
                    title: titles[index],
                    index: index,
                    fileExtension: fileExtension
                )
                let fileName = Self.uniqueFileName(
                    directory: directory,
                    base: base,
                    fileExtension: fileExtension,
                    reserved: plannedNames
                )
                plannedNames.insert(fileName)
                let liveURL = liveVideos[index].flatMap { $0.isEmpty ? nil : $0 }
                let mainCachePath = (cacheDirectory as NSString)
                    .appendingPathComponent("\(batchID)-\(index)-main.\(fileExtension)")
                let liveCachePath = liveURL == nil
                    ? nil
                    : (cacheDirectory as NSString)
                        .appendingPathComponent("\(batchID)-\(index)-live.mp4")
                let expectedMainSize = Int64(sizes[index])
                let segments = Self.segmentPlan(
                    total: expectedMainSize,
                    fileExtension: fileExtension,
                    mainCachePath: mainCachePath
                )
                items.append(
                    BackgroundDownloadItem(
                        index: index,
                        title: titles[index],
                        url: urls[index],
                        fileExtension: fileExtension,
                        liveVideoURL: liveURL,
                        destinationPath: (directory as NSString).appendingPathComponent(fileName),
                        mainCachePath: mainCachePath,
                        liveCachePath: liveCachePath,
                        expectedMainSize: expectedMainSize,
                        mainSegments: segments,
                        mainTotal: expectedMainSize
                    )
                )
            }

            let newBatch = BackgroundDownloadBatch(
                id: batchID,
                directory: directory,
                cacheDirectory: cacheDirectory,
                items: items,
                startedAt: Date()
            )
            batch = newBatch
            lastReportedBytes.removeAll()
            persist(newBatch)
            let title = newBatch.items.first?.title ?? "正在下载"
            DownloadLiveActivityController.shared.start(
                totalCount: newBatch.items.count,
                title: title
            )
            switch PHPhotoLibrary.authorizationStatus(for: .addOnly) {
            case .authorized, .limited:
                PhotoLibraryPermission.clearPendingReminder()
                enqueueTasks(for: newBatch)
            case .notDetermined:
                PhotoLibraryPermission.request { [weak self] allowed in
                    self?.queue.async {
                        guard let self, let current = self.batch,
                              current.id == newBatch.id, !current.finalized
                        else { return }
                        if allowed {
                            self.enqueueTasks(for: current)
                        } else {
                            self.failForMissingPhotoPermission(batchID: newBatch.id)
                        }
                    }
                }
            case .denied, .restricted:
                failForMissingPhotoPermission(batchID: newBatch.id)
            @unknown default:
                failForMissingPhotoPermission(batchID: newBatch.id)
            }
            accepted = true
        }
        return accepted
    }

    func cancel(batchID: String) {
        queue.async { [weak self] in
            guard let self, var current = self.batch, current.id == batchID,
                  !current.finalized
            else { return }
            current.cancelled = true
            for index in current.items.indices where !current.items[index].isTerminal {
                current.items[index].failed = true
                current.items[index].error = "下载已取消"
                self.emitFailed(
                    batchID: batchID,
                    index: current.items[index].index,
                    message: "下载已取消",
                    timedOut: false
                )
            }
            self.batch = current
            self.persist(current)
            self.cancelTasks(for: batchID)
            self.finishIfReady()
        }
    }

    private func enqueueTasks(for batch: BackgroundDownloadBatch) {
        for item in batch.items {
            if let segments = item.mainSegments, !segments.isEmpty {
                for segment in segments {
                    if !enqueueTask(
                        batchID: batch.id,
                        item: item,
                        kind: .main,
                        segment: segment
                    ) {
                        failInvalidURL(batchID: batch.id, itemIndex: item.index)
                        break
                    }
                }
            } else if !enqueueTask(
                batchID: batch.id,
                item: item,
                kind: .main,
                segment: nil
            ) {
                failInvalidURL(batchID: batch.id, itemIndex: item.index)
                continue
            }
            if item.liveVideoURL != nil &&
                !enqueueTask(
                    batchID: batch.id,
                    item: item,
                    kind: .live,
                    segment: nil
                ) {
                failInvalidURL(batchID: batch.id, itemIndex: item.index)
            }
        }
        finishIfReady()
    }

    private func enqueueTask(
        batchID: String,
        item: BackgroundDownloadItem,
        kind: DownloadTaskKind,
        segment: BackgroundDownloadPart?
    ) -> Bool {
        let rawURL = kind == .main ? item.url : item.liveVideoURL
        guard let rawURL, let url = Self.mediaURL(from: rawURL) else {
            NSLog("reels: invalid media URL for item %d", item.index)
            return false
        }
        var request = URLRequest(url: url)
        request.setValue(Self.userAgent, forHTTPHeaderField: "User-Agent")
        request.setValue(Self.referer(for: rawURL), forHTTPHeaderField: "Referer")
        request.setValue("identity", forHTTPHeaderField: "Accept-Encoding")
        if let segment {
            request.setValue(
                "bytes=\(segment.start)-\(segment.end)",
                forHTTPHeaderField: "Range"
            )
        }
        let task = session.downloadTask(with: request)
        task.taskDescription = DownloadTaskDescriptor.encode(
            batchID: batchID,
            index: item.index,
            kind: kind,
            segment: segment?.index
        )
        task.resume()
        return true
    }

    private func failInvalidURL(batchID: String, itemIndex: Int) {
        guard var current = batch, current.id == batchID,
              current.items.indices.contains(itemIndex)
        else { return }
        markFailed(
            batch: &current,
            itemIndex: itemIndex,
            message: "无效的下载地址",
            timedOut: false
        )
        batch = current
        persist(current)
        publishActivity(current)
    }

    private func cancelTasks(for batchID: String) {
        session.getAllTasks { [weak self] tasks in
            guard let self else { return }
            for task in tasks {
                guard let descriptor = DownloadTaskDescriptor(description: task.taskDescription),
                      descriptor.batchID == batchID
                else { continue }
                task.cancel()
            }
            self.queue.async {
                self.finishIfReady()
            }
        }
    }

    private func restoreState() {
        guard let data = UserDefaults.standard.data(forKey: Self.stateKey),
              let restored = try? JSONDecoder().decode(BackgroundDownloadBatch.self, from: data)
        else { return }
        batch = restored
    }

    private func cleanStaleTasks() {
        session.getAllTasks { tasks in
            for task in tasks {
                task.cancel()
            }
        }
    }

    private func updateItem(
        descriptor: DownloadTaskDescriptor,
        downloaded: Int64,
        expected: Int64,
        started: Bool
    ) {
        queue.async { [weak self] in
            guard let self, var current = self.batch, current.id == descriptor.batchID,
                  !current.finalized,
                  current.items.indices.contains(descriptor.index)
            else { return }
            var item = current.items[descriptor.index]
            guard !item.isTerminal else { return }

            let normalizedTotal = max(expected, downloaded)
            let segmentSuffix = descriptor.segment.map { "|\($0)" } ?? ""
            let reportKey = "\(descriptor.batchID)|\(item.index)|" +
                "\(descriptor.kind.rawValue)\(segmentSuffix)"
            let previous = self.lastReportedBytes[reportKey] ?? -1
            if descriptor.kind == .main {
                if let segmentIndex = descriptor.segment,
                   var segments = item.mainSegments,
                   segments.indices.contains(segmentIndex) {
                    segments[segmentIndex].downloaded = max(
                        segments[segmentIndex].downloaded,
                        downloaded
                    )
                    item.mainSegments = segments
                    item.mainDownloaded = segments.reduce(0) { $0 + $1.downloaded }
                    item.mainTotal = max(
                        item.expectedMainSize,
                        segments.reduce(0) { $0 + $1.expected }
                    )
                } else {
                    if normalizedTotal > item.mainTotal {
                        item.mainTotal = normalizedTotal
                    }
                    item.mainDownloaded = max(item.mainDownloaded, downloaded)
                }
                if started && !item.mainStartedReported {
                    item.mainStartedReported = true
                    self.emitStarted(
                        batchID: current.id,
                        index: item.index,
                        total: item.totalBytes
                    )
                }
            } else {
                if normalizedTotal > item.liveTotal {
                    item.liveTotal = normalizedTotal
                }
                item.liveDownloaded = max(item.liveDownloaded, downloaded)
                if started && !item.liveStartedReported {
                    item.liveStartedReported = true
                    self.emitStarted(
                        batchID: current.id,
                        index: item.index,
                        total: item.totalBytes
                    )
                }
            }
            current.items[descriptor.index] = item
            self.batch = current

            guard downloaded - previous >= 128 * 1024 ||
                    (normalizedTotal > 0 && downloaded >= normalizedTotal)
            else { return }
            self.lastReportedBytes[reportKey] = downloaded
            self.emitProgress(
                batchID: current.id,
                index: item.index,
                downloaded: item.downloadedBytes,
                total: item.totalBytes
            )
            self.publishActivity(current)
        }
    }

    private func handleFinishedDownload(
        descriptor: DownloadTaskDescriptor,
        location: URL,
        statusCode: Int?
    ) {
        queue.sync {
            guard var current = batch, current.id == descriptor.batchID,
                  !current.finalized,
                  current.items.indices.contains(descriptor.index)
            else { return }
            var item = current.items[descriptor.index]
            if let statusCode, !(200..<300).contains(statusCode) {
                markFailed(
                    batch: &current,
                    itemIndex: descriptor.index,
                    message: "服务器返回错误: HTTP \(statusCode)",
                    timedOut: false
                )
                batch = current
                persist(current)
                publishActivity(current)
                finishIfReady()
                return
            }
            if descriptor.segment != nil && statusCode != 206 {
                markFailed(
                    batch: &current,
                    itemIndex: descriptor.index,
                    message: "服务器不支持分段下载",
                    timedOut: false
                )
                batch = current
                persist(current)
                publishActivity(current)
                finishIfReady()
                return
            }
            let targetPath: String?
            if let segmentIndex = descriptor.segment,
               let segments = item.mainSegments,
               segments.indices.contains(segmentIndex) {
                targetPath = segments[segmentIndex].cachePath
            } else {
                targetPath = descriptor.kind == .main ? item.mainCachePath : item.liveCachePath
            }
            guard let targetPath else { return }
            let targetURL = URL(fileURLWithPath: targetPath)
            let fileManager = FileManager.default
            do {
                try fileManager.removeItem(at: targetURL)
            } catch {
                // 目标文件不存在时无需处理。
            }
            do {
                try fileManager.moveItem(at: location, to: targetURL)
            } catch {
                markFailed(
                    batch: &current,
                    itemIndex: descriptor.index,
                    message: "保存后台下载文件失败: \(error.localizedDescription)",
                    timedOut: false
                )
                batch = current
                persist(current)
                finishIfReady()
                return
            }

            if let segmentIndex = descriptor.segment,
               var segments = item.mainSegments,
               segments.indices.contains(segmentIndex) {
                segments[segmentIndex].finished = true
                segments[segmentIndex].downloaded = max(
                    segments[segmentIndex].downloaded,
                    Self.fileSize(at: targetPath)
                )
                item.mainSegments = segments
                item.mainDownloaded = segments.reduce(0) { $0 + $1.downloaded }
                if segments.allSatisfy(\.finished) {
                    if let error = Self.mergeSegments(&item) {
                        markFailed(
                            batch: &current,
                            itemIndex: descriptor.index,
                            message: "合并分段下载失败: \(error)",
                            timedOut: false
                        )
                    } else {
                        item.mainFinished = true
                        item.mainDownloaded = max(
                            item.mainDownloaded,
                            Self.fileSize(at: item.mainCachePath)
                        )
                        item.mainTotal = max(item.mainTotal, item.mainDownloaded)
                    }
                }
            } else if descriptor.kind == .main {
                item.mainFinished = true
                item.mainDownloaded = max(
                    item.mainDownloaded,
                    Self.fileSize(at: targetPath)
                )
                if item.mainTotal == 0 {
                    item.mainTotal = item.mainDownloaded
                }
            } else {
                item.liveFinished = true
                item.liveDownloaded = max(
                    item.liveDownloaded,
                    Self.fileSize(at: targetPath)
                )
                if item.liveTotal == 0 {
                    item.liveTotal = item.liveDownloaded
                }
            }
            current.items[descriptor.index] = item
            batch = current
            persist(current)
            publishActivity(current)
            finalizeIfReady(itemIndex: descriptor.index)
        }
    }

    private static func mergeSegments(_ item: inout BackgroundDownloadItem) -> String? {
        guard let segments = item.mainSegments, !segments.isEmpty else { return nil }
        let fileManager = FileManager.default
        try? fileManager.removeItem(atPath: item.mainCachePath)
        guard fileManager.createFile(atPath: item.mainCachePath, contents: nil) else {
            return "无法创建目标文件"
        }
        do {
            let output = try FileHandle(forWritingTo: URL(fileURLWithPath: item.mainCachePath))
            defer { try? output.close() }
            for segment in segments.sorted(by: { $0.index < $1.index }) {
                let input = try FileHandle(forReadingFrom: URL(fileURLWithPath: segment.cachePath))
                defer { try? input.close() }
                while let data = try input.read(upToCount: 1024 * 1024), !data.isEmpty {
                    try output.write(contentsOf: data)
                }
                try? fileManager.removeItem(atPath: segment.cachePath)
            }
        } catch {
            return error.localizedDescription
        }
        item.mainSegments = nil
        return nil
    }

    private func finalizeIfReady(itemIndex: Int) {
        guard var current = batch, current.items.indices.contains(itemIndex) else { return }
        let item = current.items[itemIndex]
        guard !item.isTerminal, item.mainFinished,
              !item.hasLiveVideo || item.liveFinished
        else { return }
        guard PhotoLibraryPermission.isAllowed() else {
            current.items[itemIndex].failed = true
            current.items[itemIndex].error = PhotoLibraryPermission.requiredMessage
            current.permissionDenied = true
            batch = current
            persist(current)
            emitFailed(
                batchID: current.id,
                index: item.index,
                message: PhotoLibraryPermission.requiredMessage,
                timedOut: false
            )
            remindPhotoPermissionIfNeeded()
            finishIfReady()
            return
        }
        current.items[itemIndex].finalized = true
        batch = current
        persist(current)

        let batchID = current.id
        let mainPath = item.mainCachePath
        let livePath = item.liveCachePath
        let destination = item.destinationPath
        let title = item.title
        pendingFinalizations += 1
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let saved = Self.finalizeDownloadedFile(mainPath, livePath: livePath)
            self?.queue.async {
                guard let self, var current = self.batch, current.id == batchID,
                      current.items.indices.contains(itemIndex)
                else {
                    self?.pendingFinalizations = max((self?.pendingFinalizations ?? 1) - 1, 0)
                    self?.completeBackgroundEventsIfReady()
                    return
                }
                self.pendingFinalizations = max(self.pendingFinalizations - 1, 0)
                if saved {
                    current.items[itemIndex].saved = true
                    self.emitCompleted(
                        batchID: batchID,
                        index: current.items[itemIndex].index,
                        path: destination
                    )
                } else {
                    current.items[itemIndex].failed = true
                    current.items[itemIndex].error = "保存到相册失败"
                    self.emitFailed(
                        batchID: batchID,
                        index: current.items[itemIndex].index,
                        message: "保存到相册失败",
                        timedOut: false
                    )
                }
                self.batch = current
                self.persist(current)
                self.publishActivity(current, title: title)
                self.finishIfReady()
                self.completeBackgroundEventsIfReady()
            }
        }
    }

    private func markFailed(
        batch: inout BackgroundDownloadBatch,
        itemIndex: Int,
        message: String,
        timedOut: Bool
    ) {
        guard batch.items.indices.contains(itemIndex),
              !batch.items[itemIndex].isTerminal
        else { return }
        batch.items[itemIndex].failed = true
        batch.items[itemIndex].error = message
        emitFailed(
            batchID: batch.id,
            index: batch.items[itemIndex].index,
            message: message,
            timedOut: timedOut
        )
    }

    private func finishIfReady() {
        guard var current = batch, !current.finalized,
              pendingFinalizations == 0,
              current.items.allSatisfy({ $0.isTerminal })
        else { return }
        current.finalized = true
        batch = current
        persist(current)
        emitFinished(batchID: current.id)

        let success = current.items.filter { $0.saved }.count
        let total = current.items.count
        let downloaded = current.items.reduce(Int64(0)) { $0 + $1.downloadedBytes }
        let totalBytes = current.items.reduce(Int64(0)) { $0 + $1.totalBytes }
        let progress = totalBytes > 0 ? Double(downloaded) / Double(totalBytes) : 0
        let phase: DownloadActivityAttributes.Phase = current.cancelled
            ? .cancelled
            : (success > 0 ? .completed : .failed)
        let title: String
        if current.permissionDenied && success == 0 {
            title = "请允许相册权限"
        } else if success > 0 {
            title = "已保存 \(success)/\(total) 个资源"
        } else {
            title = current.cancelled ? "下载已取消" : "下载失败"
        }
        DownloadLiveActivityController.shared.finish(
            phase: phase,
            title: title,
            progress: success > 0 ? max(progress, 1) : progress,
            completedCount: success,
            totalCount: total,
            downloadedBytes: downloaded,
            totalBytes: totalBytes
        )

        let fileManager = FileManager.default
        for item in current.items where !item.saved {
            try? fileManager.removeItem(atPath: item.mainCachePath)
            if let liveCachePath = item.liveCachePath {
                try? fileManager.removeItem(atPath: liveCachePath)
            }
            for segment in item.mainSegments ?? [] {
                try? fileManager.removeItem(atPath: segment.cachePath)
            }
        }
        batch = nil
        UserDefaults.standard.removeObject(forKey: Self.stateKey)
        lastReportedBytes.removeAll()
        completeBackgroundEventsIfReady()
    }

    private func completeBackgroundEventsIfReady() {
        guard self.batch == nil, pendingFinalizations == 0,
              let completion = backgroundCompletionHandler
        else { return }
        backgroundCompletionHandler = nil
        DispatchQueue.main.async {
            completion()
        }
    }

    private func failForMissingPhotoPermission(batchID: String) {
        guard var current = batch, current.id == batchID, !current.finalized else { return }
        current.permissionDenied = true
        for index in current.items.indices where !current.items[index].isTerminal {
            current.items[index].failed = true
            current.items[index].error = PhotoLibraryPermission.requiredMessage
            emitFailed(
                batchID: batchID,
                index: current.items[index].index,
                message: PhotoLibraryPermission.requiredMessage,
                timedOut: false
            )
        }
        batch = current
        persist(current)
        remindPhotoPermissionIfNeeded()
        finishIfReady()
    }

    private func remindPhotoPermissionIfNeeded() {
        guard var current = batch, !current.permissionReminderSent else { return }
        current.permissionReminderSent = true
        batch = current
        persist(current)
        PhotoLibraryPermission.remind()
    }

    private func publishActivity(
        _ current: BackgroundDownloadBatch,
        title: String? = nil
    ) {
        let completed = current.items.filter { $0.saved }.count
        let downloaded = current.items.reduce(Int64(0)) { $0 + $1.downloadedBytes }
        let totalBytes = current.items.reduce(Int64(0)) { $0 + $1.totalBytes }
        let progress = totalBytes > 0 ? Double(downloaded) / Double(totalBytes) : 0
        let currentItem = current.items.first {
            !$0.isTerminal && ($0.mainDownloaded > 0 || $0.mainTotal > 0)
        } ?? current.items.first { !$0.isTerminal } ?? current.items.last
        DownloadLiveActivityController.shared.update(
            title: title ?? currentItem?.title ?? "正在下载",
            progress: progress,
            completedCount: completed,
            totalCount: current.items.count,
            downloadedBytes: downloaded,
            totalBytes: totalBytes
        )
    }

    private func persist(_ batch: BackgroundDownloadBatch) {
        guard let data = try? JSONEncoder().encode(batch) else { return }
        UserDefaults.standard.set(data, forKey: Self.stateKey)
    }

    private func emitStarted(batchID: String, index: Int, total: Int64) {
        batchID.withCString { id in
            reels_ios_download_event_started(id, UInt32(index), UInt64(max(total, 0)))
        }
    }

    private func emitProgress(batchID: String, index: Int, downloaded: Int64, total: Int64) {
        batchID.withCString { id in
            reels_ios_download_event_progress(
                id,
                UInt32(index),
                UInt64(max(downloaded, 0)),
                UInt64(max(total, 0))
            )
        }
    }

    private func emitCompleted(batchID: String, index: Int, path: String) {
        batchID.withCString { id in
            path.withCString { pathPointer in
                reels_ios_download_event_completed(id, UInt32(index), pathPointer)
            }
        }
    }

    private func emitFailed(
        batchID: String,
        index: Int,
        message: String,
        timedOut: Bool
    ) {
        batchID.withCString { id in
            message.withCString { messagePointer in
                reels_ios_download_event_failed(
                    id,
                    UInt32(index),
                    messagePointer,
                    timedOut
                )
            }
        }
    }

    private func emitFinished(batchID: String) {
        batchID.withCString { id in
            reels_ios_download_event_finished(id)
        }
    }

    private static func finalizeDownloadedFile(_ path: String, livePath: String?) -> Bool {
        path.withCString { pathPointer in
            if let livePath {
                return livePath.withCString { livePointer in
                    reels_ios_finalize_downloaded_file(pathPointer, livePointer)
                }
            }
            return reels_ios_finalize_downloaded_file(pathPointer, nil)
        }
    }

    private static func referer(for url: String) -> String {
        if url.contains("xhscdn.com") || url.contains("xiaohongshu.com") {
            return "https://www.xiaohongshu.com/"
        }
        return "https://www.douyin.com/"
    }

    private static func segmentPlan(
        total: Int64,
        fileExtension: String,
        mainCachePath: String
    ) -> [BackgroundDownloadPart]? {
        let videoExtensions = ["mp4", "mov", "m4v", "webm"]
        guard videoExtensions.contains(fileExtension.lowercased()),
              total >= segmentedDownloadThreshold
        else { return nil }
        let count = segmentCount
        let segmentSize = total / Int64(count)
        guard segmentSize > 0 else { return nil }
        return (0..<count).map { index in
            let start = Int64(index) * segmentSize
            let end = index == count - 1 ? total - 1 : start + segmentSize - 1
            return BackgroundDownloadPart(
                index: index,
                start: start,
                end: end,
                cachePath: "\(mainCachePath).part\(index)"
            )
        }
    }

    private static func mediaURL(from raw: String) -> URL? {
        var value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        value = value
            .replacingOccurrences(of: "\r", with: "")
            .replacingOccurrences(of: "\n", with: "")
            .replacingOccurrences(of: "&amp;", with: "&")
        guard !value.isEmpty else { return nil }
        if value.hasPrefix("//") {
            value = "https:" + value
        }

        var parsed = URL(string: value)
        if parsed?.scheme == nil {
            parsed = URL(string: "https://" + value)
        }
        guard let parsed,
              let scheme = parsed.scheme?.lowercased(),
              scheme == "http" || scheme == "https",
              let host = parsed.host,
              !host.isEmpty
        else { return nil }
        return parsed
    }

    private static func fileSize(at path: String) -> Int64 {
        let attributes = try? FileManager.default.attributesOfItem(atPath: path)
        return (attributes?[.size] as? NSNumber)?.int64Value ?? 0
    }

    private static func downloadBaseName(
        title: String,
        index: Int,
        fileExtension: String
    ) -> String {
        var base = safeFileName(title, maxChars: 80, maxBytes: 180)
        let suffix = ".\(fileExtension.lowercased())"
        if base.lowercased().hasSuffix(suffix) {
            base.removeLast(suffix.count)
        } else if base.lowercased().hasSuffix(".mp4") {
            base.removeLast(4)
        }
        base = base.trimmingCharacters(in: CharacterSet(charactersIn: " ."))
        return base.isEmpty ? "抖音视频_\(index + 1)" : base
    }

    private static func uniqueFileName(
        directory: String,
        base: String,
        fileExtension: String,
        reserved: Set<String>
    ) -> String {
        var candidate = base
        var counter = 1
        while true {
            let name = "\(candidate).\(fileExtension)"
            let path = (directory as NSString).appendingPathComponent(name)
            if !FileManager.default.fileExists(atPath: path) && !reserved.contains(name) {
                return name
            }
            candidate = "\(base)_\(counter)"
            counter += 1
        }
    }

    private static func safeFileName(_ input: String, maxChars: Int, maxBytes: Int) -> String {
        let invalid = CharacterSet(charactersIn: "<>:\"/\\|?*")
        var collapsed = ""
        var lastWasSpace = false
        for scalar in input.trimmingCharacters(in: .whitespacesAndNewlines).unicodeScalars {
            if CharacterSet.whitespacesAndNewlines.contains(scalar) {
                if !lastWasSpace {
                    collapsed.unicodeScalars.append(" ")
                    lastWasSpace = true
                }
                continue
            }
            lastWasSpace = false
            if invalid.contains(scalar) || (scalar.value < 0x20) {
                collapsed.append("_")
            } else {
                collapsed.unicodeScalars.append(scalar)
            }
        }
        while collapsed.hasSuffix(".") || collapsed.hasSuffix(" ") {
            collapsed.removeLast()
        }

        var result = ""
        var charCount = 0
        var byteCount = 0
        for character in collapsed {
            let bytes = character.utf8.count
            if charCount >= maxChars || byteCount + bytes > maxBytes {
                break
            }
            result.append(character)
            charCount += 1
            byteCount += bytes
        }
        result = result.trimmingCharacters(in: .whitespaces)
        if result.isEmpty {
            return "download"
        }
        let reserved = [
            "con", "prn", "aux", "nul",
            "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
            "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        ]
        if reserved.contains(result.lowercased()) {
            return "_\(result)"
        }
        return result
    }
}

@available(iOS 16.1, *)
extension ReelsBackgroundDownloader: URLSessionDownloadDelegate {
    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        guard let descriptor = DownloadTaskDescriptor(
            description: downloadTask.taskDescription
        ) else { return }
        updateItem(
            descriptor: descriptor,
            downloaded: totalBytesWritten,
            expected: totalBytesExpectedToWrite,
            started: true
        )
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        guard let descriptor = DownloadTaskDescriptor(
            description: downloadTask.taskDescription
        ) else { return }
        let statusCode = (downloadTask.response as? HTTPURLResponse)?.statusCode
        handleFinishedDownload(
            descriptor: descriptor,
            location: location,
            statusCode: statusCode
        )
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard error != nil,
              let descriptor = DownloadTaskDescriptor(description: task.taskDescription)
        else { return }
        queue.async { [weak self] in
            guard let self, var current = self.batch,
                  current.id == descriptor.batchID, !current.finalized,
                  current.items.indices.contains(descriptor.index)
            else { return }
            guard !current.items[descriptor.index].isTerminal else { return }
            let nsError = error as NSError?
            let timedOut = nsError?.code == NSURLErrorTimedOut
            let message: String
            if current.cancelled || nsError?.code == NSURLErrorCancelled {
                message = "下载已取消"
            } else if let error {
                message = error.localizedDescription
            } else {
                message = "下载失败"
            }
            self.markFailed(
                batch: &current,
                itemIndex: descriptor.index,
                message: message,
                timedOut: timedOut
            )
            self.batch = current
            self.persist(current)
            self.publishActivity(current)
            self.finishIfReady()
        }
    }

    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        queue.async { [weak self] in
            self?.completeBackgroundEventsIfReady()
        }
    }
}

@_cdecl("reels_ios_background_download_start")
public func reelsBackgroundDownloadStart(
    _ batchID: UnsafePointer<CChar>?,
    _ urls: UnsafePointer<UnsafePointer<CChar>?>?,
    _ titles: UnsafePointer<UnsafePointer<CChar>?>?,
    _ extensions: UnsafePointer<UnsafePointer<CChar>?>?,
    _ liveVideos: UnsafePointer<UnsafePointer<CChar>?>?,
    _ sizes: UnsafePointer<UInt64>?,
    _ count: Int,
    _ directory: UnsafePointer<CChar>?,
    _ cacheDirectory: UnsafePointer<CChar>?
) -> Bool {
    guard let batchID, let urls, let titles, let extensions, let liveVideos, let sizes,
          let directory, let cacheDirectory, count > 0
    else { return false }
    guard #available(iOS 16.1, *) else { return false }

    func string(at pointer: UnsafePointer<CChar>?) -> String? {
        pointer.map { String(cString: $0) }
    }

    var urlValues: [String] = []
    var titleValues: [String] = []
    var extensionValues: [String?] = []
    var liveValues: [String?] = []
    var sizeValues: [UInt64] = []
    for index in 0..<count {
        guard let url = string(at: urls[index]), let title = string(at: titles[index]) else {
            return false
        }
        urlValues.append(url)
        titleValues.append(title)
        extensionValues.append(string(at: extensions[index]))
        liveValues.append(string(at: liveVideos[index]))
        sizeValues.append(sizes[index])
    }
    return ReelsBackgroundDownloader.shared.start(
        batchID: String(cString: batchID),
        urls: urlValues,
        titles: titleValues,
        extensions: extensionValues,
        liveVideos: liveValues,
        sizes: sizeValues,
        directory: String(cString: directory),
        cacheDirectory: String(cString: cacheDirectory)
    )
}

@_cdecl("reels_ios_background_download_cancel")
public func reelsBackgroundDownloadCancel(_ batchID: UnsafePointer<CChar>?) {
    guard let batchID, #available(iOS 16.1, *) else { return }
    ReelsBackgroundDownloader.shared.cancel(batchID: String(cString: batchID))
}
