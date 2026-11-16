import UIKit

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
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    private var displayLink: CADisplayLink?
    private var launchOverlay: UIView?
    private var launchLogo: UIImageView?
    private var appearanceTraitObserver: AppearanceTraitObserver?
    private var launchFramesPresented = 0
    private var launchAnimationSettled = false
    private var launchOverlayRemoved = false
    private var iconSyncInFlight = false

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        reels_ios_start()
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

    private func updateAppIcon(_ systemDark: Bool) {
        let application = UIApplication.shared
        guard application.supportsAlternateIcons, !iconSyncInFlight else { return }
        let iconName = systemDark ? "AppIconDark" : "AppIconLight"
        guard application.alternateIconName != iconName else { return }
        iconSyncInFlight = true
        let applyIcon = { [weak self] in
            application.setAlternateIconName(iconName) { error in
                DispatchQueue.main.async {
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
        gpui_ios_did_become_active(nil)
    }

    func applicationWillResignActive(_ application: UIApplication) {
        gpui_ios_will_resign_active(nil)
    }

    func applicationDidEnterBackground(_ application: UIApplication) {
        gpui_ios_did_enter_background(nil)
    }

    func applicationWillTerminate(_ application: UIApplication) {
        gpui_ios_will_terminate(nil)
    }
}
