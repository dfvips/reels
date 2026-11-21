import Photos
import UIKit
import UserNotifications

enum PhotoLibraryPermission {
    static let requiredMessage =
        "请在“设置 > 短视频解析 > 照片”中允许访问相册，然后重新下载。"
    static let notificationIdentifier = "reels.photo-permission-required"
    private static let pendingReminderKey = "reels.photoPermissionReminderPending"

    private static var presentedAlert = false

    static func resetReminderForNewAttempt() {
        DispatchQueue.main.async {
            presentedAlert = false
        }
    }

    static func isAllowed(_ status: PHAuthorizationStatus? = nil) -> Bool {
        let status = status ?? PHPhotoLibrary.authorizationStatus(for: .addOnly)
        switch status {
        case .authorized, .limited:
            return true
        case .notDetermined, .denied, .restricted:
            return false
        @unknown default:
            return false
        }
    }

    static func request(completion: @escaping (Bool) -> Void) {
        PHPhotoLibrary.requestAuthorization(for: .addOnly) { status in
            DispatchQueue.main.async {
                if isAllowed(status) {
                    presentedAlert = false
                    clearPendingReminder()
                }
                completion(isAllowed(status))
            }
        }
    }

    static func remind() {
        UserDefaults.standard.set(true, forKey: pendingReminderKey)
        ReelsNotificationCenter.shared.notifyPhotoPermissionRequired()
        presentPendingAlertIfNeeded()
    }

    static func presentPendingAlertIfNeeded() {
        DispatchQueue.main.async {
            guard UserDefaults.standard.bool(forKey: pendingReminderKey) else { return }
            if isAllowed() {
                clearPendingReminder()
                return
            }
            guard
                  !presentedAlert,
                  UIApplication.shared.applicationState == .active,
                  let presenter = topViewController()
            else { return }
            presentedAlert = true
            let alert = UIAlertController(
                title: "需要相册权限",
                message: requiredMessage,
                preferredStyle: .alert
            )
            alert.addAction(UIAlertAction(title: "稍后", style: .cancel))
            alert.addAction(
                UIAlertAction(title: "去设置", style: .default) { _ in
                    openSettings()
                }
            )
            presenter.present(alert, animated: true)
        }
    }

    static func clearPendingReminder() {
        UserDefaults.standard.removeObject(forKey: pendingReminderKey)
    }

    static func openSettings() {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        DispatchQueue.main.async {
            UIApplication.shared.open(url)
        }
    }

    private static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap {
            $0 as? UIWindowScene
        }
        let windows = scenes.flatMap(\.windows)
        guard let window = windows.first(where: \.isKeyWindow)
            ?? windows.first
            ?? UIApplication.shared.keyWindow,
            var controller = window.rootViewController
        else { return nil }
        while let presented = controller.presentedViewController {
            controller = presented
        }
        return controller
    }
}

final class ReelsNotificationCenter {
    static let shared = ReelsNotificationCenter()

    private init() {}

    func notifyPhotoPermissionRequired() {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .notDetermined:
                center.requestAuthorization(options: [.alert, .sound, .provisional]) {
                    _, _ in self.deliver()
                }
            case .denied:
                return
            case .authorized, .provisional, .ephemeral:
                self.deliver()
            @unknown default:
                self.deliver()
            }
        }
    }

    private func deliver() {
        let content = UNMutableNotificationContent()
        content.title = "无法保存到相册"
        content.body = PhotoLibraryPermission.requiredMessage
        content.sound = .default
        content.userInfo = ["reels.openSettings": true]
        let request = UNNotificationRequest(
            identifier: PhotoLibraryPermission.notificationIdentifier,
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
