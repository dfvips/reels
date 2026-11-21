import UIKit
import UserNotifications

private final class AppearanceTraitObserver: UIView {
    var onUserInterfaceStyleChanged: (() -> Void)?

    override func traitCollectionDidChange(_ previousTraitCollection: UITraitCollection?) {
        super.traitCollectionDidChange(previousTraitCollection)
        guard let previousTraitCollection,
              traitCollection.userInterfaceStyle != previousTraitCollection.userInterfaceStyle
        else { return }
        onUserInterfaceStyleChanged?()
    }
}

@main
class AppDelegate: UIResponder, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    var window: UIWindow?
    private var displayLink: CADisplayLink?
    private var launchOverlay: UIView?
    private var launchLogo: UIImageView?
    private var appearanceTraitObserver: AppearanceTraitObserver?
    private var launchFramesPresented = 0
    private var launchAnimationSettled = false
    private var launchOverlayRemoved = false
    private var iconSyncInFlight = false
    private var isFirstLaunch = false
    private static let launchedBeforeKey = "reels.hasLaunchedBefore"
    private static let appIconSchemeKey = "reels.appIconScheme"

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // 首次安装启动时不同步图标，避免系统立刻弹出“已更换图标”提示。
        let defaults = UserDefaults.standard
        isFirstLaunch = !defaults.bool(forKey: Self.launchedBeforeKey)
        if isFirstLaunch {
            defaults.set(true, forKey: Self.launchedBeforeKey)
        }
        UNUserNotificationCenter.current().delegate = self
        reels_ios_start()
        if #available(iOS 16.1, *) {
            ReelsBackgroundDownloader.shared.activate()
        }
        DispatchQueue.main.async { [weak self] in
            self?.showLaunchOverlay()
        }
        scheduleSystemAppearanceRefresh()
        let link = CADisplayLink(target: self, selector: #selector(renderFrame))
        link.add(to: .main, forMode: .common)
        displayLink = link
        return true
    }

    @objc private func renderFrame() {
        let window = gpui_ios_get_window()
        if window != nil {
            gpui_ios_request_frame(window)
            countLaunchFrame()
        }
    }

    private func showLaunchOverlay() {
        guard launchOverlay == nil, let hostWindow = currentKeyWindow() else { return }
        installAppearanceTraitObserver(in: hostWindow)
        let overlay = UIView(frame: hostWindow.bounds)
        overlay.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        overlay.backgroundColor = UIColor(named: "LaunchBackground") ?? launchBackgroundColor()
        overlay.isUserInteractionEnabled = false

        let logo = UIImageView(image: UIImage(named: "SplashLogo"))
        logo.contentMode = .scaleAspectFit
        logo.translatesAutoresizingMaskIntoConstraints = false
        overlay.addSubview(logo)
        NSLayoutConstraint.activate([
            logo.widthAnchor.constraint(equalToConstant: 144),
            logo.heightAnchor.constraint(equalTo: logo.widthAnchor),
            logo.centerXAnchor.constraint(equalTo: overlay.centerXAnchor),
            logo.centerYAnchor.constraint(equalTo: overlay.centerYAnchor),
        ])

        hostWindow.addSubview(overlay)
        launchOverlay = overlay
        launchLogo = logo

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.55) { [weak self] in
            self?.launchAnimationSettled = true
            self?.removeLaunchOverlayIfReady()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) { [weak self] in
            self?.removeLaunchOverlay()
        }
    }

    private func countLaunchFrame() {
        guard !launchOverlayRemoved else { return }
        launchFramesPresented += 1
        removeLaunchOverlayIfReady()
    }

    private func removeLaunchOverlayIfReady() {
        guard launchAnimationSettled, launchFramesPresented >= 4 else { return }
        removeLaunchOverlay()
    }

    private func removeLaunchOverlay() {
        guard !launchOverlayRemoved, let overlay = launchOverlay else { return }
        launchOverlayRemoved = true
        UIView.animate(
            withDuration: 0.28,
            delay: 0,
            options: [.curveEaseInOut, .allowUserInteraction],
            animations: {
                overlay.alpha = 0
                self.launchLogo?.transform = CGAffineTransform(scaleX: 1.06, y: 1.06)
            },
            completion: { [weak self] _ in
                overlay.removeFromSuperview()
                self?.launchOverlay = nil
                self?.launchLogo = nil
            }
        )
    }

    private func launchBackgroundColor() -> UIColor {
        if #available(iOS 13.0, *) {
            return UIColor { trait in
                trait.userInterfaceStyle == .dark
                    ? UIColor(red: 20 / 255, green: 20 / 255, blue: 20 / 255, alpha: 1)
                    : .white
            }
        }
        return .white
    }

    private func refreshSystemAppearance() {
        if let hostWindow = currentKeyWindow() {
            installAppearanceTraitObserver(in: hostWindow)
        }
        let screenStyle = UIScreen.main.traitCollection.userInterfaceStyle
        let style = screenStyle != .unspecified
            ? screenStyle
            : currentKeyWindow()?.traitCollection.userInterfaceStyle
                ?? UITraitCollection.current.userInterfaceStyle
        let systemDark = style == .dark
        updateAppIcon(systemDark)
        DispatchQueue.main.async {
            reels_ios_refresh_system_appearance(systemDark)
        }
    }

    /// 当前图标对应的深浅色：备用图标优先，否则取上次记录（默认图标为浅色）。
    private func currentAppIconScheme() -> String {
        switch UIApplication.shared.alternateIconName {
        case "AppIconDark":
            return "dark"
        case "AppIconLight":
            return "light"
        default:
            return UserDefaults.standard.string(forKey: Self.appIconSchemeKey) ?? "light"
        }
    }

    private func updateAppIcon(_ systemDark: Bool) {
        if isFirstLaunch {
            // 首次启动不切换图标，但记录当前实际状态，供下次启动判断。
            UserDefaults.standard.set(currentAppIconScheme(), forKey: Self.appIconSchemeKey)
            return
        }
        let application = UIApplication.shared
        guard application.supportsAlternateIcons, !iconSyncInFlight else { return }
        guard let window = currentKeyWindow() else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
                self?.updateAppIcon(systemDark)
            }
            return
        }
        let style = window.traitCollection.userInterfaceStyle
        guard style != .unspecified else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
                self?.updateAppIcon(systemDark)
            }
            return
        }
        // 跟随 App 实际展示的主题，而非只按系统外观。
        let appDark = reels_ios_app_theme_is_dark(style == .dark)
        let desiredScheme = appDark ? "dark" : "light"
        guard currentAppIconScheme() != desiredScheme else { return }
        let iconName = appDark ? "AppIconDark" : "AppIconLight"
        iconSyncInFlight = true
        let applyIcon = { [weak self] in
            application.setAlternateIconName(iconName) { error in
                DispatchQueue.main.async {
                    if error == nil {
                        UserDefaults.standard.set(
                            desiredScheme,
                            forKey: Self.appIconSchemeKey
                        )
                    }
                    self?.iconSyncInFlight = false
                }
                if let error {
                    NSLog("Failed to update alternate app icon: %@", error.localizedDescription)
                }
            }
        }
        if application.applicationState == .active {
            applyIcon()
        } else {
            DispatchQueue.main.async {
                applyIcon()
            }
        }
    }

    private func installAppearanceTraitObserver(in hostWindow: UIWindow) {
        guard appearanceTraitObserver == nil else { return }
        let observer = AppearanceTraitObserver(frame: CGRect(x: 0, y: 0, width: 1, height: 1))
        observer.backgroundColor = .clear
        observer.alpha = 0.001
        observer.isUserInteractionEnabled = false
        observer.onUserInterfaceStyleChanged = { [weak self] in
            self?.scheduleSystemAppearanceRefresh()
        }
        hostWindow.addSubview(observer)
        appearanceTraitObserver = observer
    }

    private func scheduleSystemAppearanceRefresh() {
        DispatchQueue.main.async { [weak self] in
            self?.refreshSystemAppearance()
        }
    }

    private func currentKeyWindow() -> UIWindow? {
        if #available(iOS 13.0, *) {
            let windows = UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap { $0.windows }
            return windows.first { $0.isKeyWindow } ?? windows.first
        }
        return UIApplication.shared.keyWindow
    }

    func applicationWillEnterForeground(_ application: UIApplication) {
        scheduleSystemAppearanceRefresh()
        gpui_ios_will_enter_foreground(nil)
    }

    func applicationDidBecomeActive(_ application: UIApplication) {
        scheduleSystemAppearanceRefresh()
        updateAppIcon(UIScreen.main.traitCollection.userInterfaceStyle == .dark)
        PhotoLibraryPermission.presentPendingAlertIfNeeded()
        gpui_ios_did_become_active(nil)
    }

    func applicationWillResignActive(_ application: UIApplication) {
        gpui_ios_will_resign_active(nil)
    }

    func applicationDidEnterBackground(_ application: UIApplication) {
        gpui_ios_did_enter_background(nil)
    }

    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        guard identifier == "com.dreamfly.reels.background-downloads" else {
            completionHandler()
            return
        }
        if #available(iOS 16.1, *) {
            ReelsBackgroundDownloader.shared.handleEventsForBackgroundURLSession(
                completionHandler: completionHandler
            )
        } else {
            completionHandler()
        }
    }

    func applicationWillTerminate(_ application: UIApplication) {
        gpui_ios_will_terminate(nil)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if response.notification.request.identifier ==
            PhotoLibraryPermission.notificationIdentifier {
            PhotoLibraryPermission.openSettings()
        }
        completionHandler()
    }
}
