// ============================================================
//  Bottle Helper — مساعد القارورة (macOS)
//  ------------------------------------------------------------
//  تطبيق macOS صغير يُثبَّت **داخل القارورة** باسم
//  "Jisr Helper (<tag>).app" فيُسجَّل كمعالج Apple Events، فيلتقط
//  أحداث Open/Print/URL من تطبيقات ويندوز ويعيد توجيهها لتطبيقنا.
//
//  يُبنى بـ:  swiftc -O BottleHelper.swift -o helper
//  (أو: xcodebuild). البناء يحتاج Xcode CLT على جهاز الماك.
// ============================================================
import AppKit
import Foundation

///_events-log.txt داخل القارورة — نقرأه من التطبيق الأم.
let eventsLogName = "_bottle-helper-events.log"

/// وسم القارورة (tag) — يجده التطبيق الأم من مسار التطبيق.
let bottleTag = Bundle.main.bundleIdentifier?
    .replacingOccurrences(of: "com.jisr.bottlehelper.", with: "")
    .replacingOccurrences(of: ".", with: "-") ?? "default"

/// مسار مجلد القارورة: التطبيق داخل
///   <prefix>/Applications/Jisr Helper (<tag>).app
/// ⇒ dirname(dirname(dirname(path))) = <prefix>
var prefix: URL {
    let appPath = Bundle.main.bundleURL
    return appPath
        .deletingLastPathComponent()   // Contents/MacOS
        .deletingLastPathComponent()   // Contents
        .deletingLastPathComponent()   // Jisr Helper (tag).app
        .deletingLastPathComponent()   // Applications
}

func log(_ message: String) {
    let line = "\(Date().timeIntervalSince1970)\t\(message)\n"
    let target = prefix.appendingPathComponent(eventsLogName)
    if let handle = try? FileHandle(forWritingTo: target) {
        handle.seekToEndOfFile()
        handle.write(Data(line.utf8))
        try? handle.close()
    } else {
        try? Data(line.utf8).write(to: target)
    }
}

/// يعيد التوجيه إلى التطبيق الأم عبر URL مخصّص.
func forwardToMainApp(_ path: String) {
    guard let url = URL(string: "jisr://open?path=\(path.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "")") else {
        return
    }
    log("forward \(url.absoluteString)")
    NSWorkspace.shared.open(url)
}

final class Helper: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        log("helper started (tag=\(bottleTag))")
        NSAppleEventManager.shared().setEventHandler(
            self,
            andSelector: #selector(handleAppleEvent(with:reply:)),
            forEventClass: AEEventClass(kInternetEventClass),
            andEventID: AEEventID(kAEGetURL)
        )
        NSAppleEventManager.shared().setEventHandler(
            self,
            andSelector: #selector(handleAppleEvent(with:reply:)),
            forEventClass: AEEventClass(kCoreEventClass),
            andEventID: AEEventID(kAEOpenDocuments)
        )
        NSAppleEventManager.shared().setEventHandler(
            self,
            andSelector: #selector(handleAppleEvent(with:reply:)),
            forEventClass: AEEventClass(kCoreEventClass),
            andEventID: AEEventID(kAEPrint)
        )
    }

    @objc func handleAppleEvent(with event: NSAppleEventDescriptor, reply: NSAppleEventDescriptor) {
        guard let eventData = event.paramDescriptor(forKeyword: keyDirectObject)?.data else {
            return
        }
        // نص مالـ URL أو قائمة مسارات.
        let payload = String(data: eventData, encoding: .utf8) ?? ""
        log("event \(event.eventID) payload=\(payload)")
        let path = payload
            .split(separator: "\r")
            .first
            .map(String.init)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? payload
        if !path.isEmpty {
            forwardToMainApp(path)
        }
    }
}

let app = NSApplication.shared
let delegate = Helper()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
