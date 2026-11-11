import UIKit

@main
class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    private var displayLink: CADisplayLink?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        douyin_parser_ios_start()
        let link = CADisplayLink(target: self, selector: #selector(renderFrame))
        link.add(to: .main, forMode: .common)
        displayLink = link
        return true
    }

    @objc private func renderFrame() {
        let window = gpui_ios_get_window()
        if window != nil {
            gpui_ios_request_frame(window)
        }
    }

    func applicationWillEnterForeground(_ application: UIApplication) {
        gpui_ios_will_enter_foreground(nil)
    }

    func applicationDidBecomeActive(_ application: UIApplication) {
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
