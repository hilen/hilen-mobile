//
//  SystemInput.swift
//  Hilen
//
//  The hands of a UI test that needs the real system: a tap on the screen, a tap
//  on a key of the screen keyboard. The engine cannot send those to itself, the
//  keyboard is drawn by another process. So the engine test connects here over a
//  local socket and asks, one line per request, and this side answers `ok` or
//  `error` after the action is done.
//
//  An agent session of `make sim` drives a running app through the same
//  socket, see `build/ios/sim.py` of hilen.
//
//  Requests, all positions in screen points:
//
//    tap <x> <y>          a tap on the screen
//    double <x> <y>       2 taps, a double tap
//    hold <x> <y> [s]     a finger held down, 1 s when no time is given
//    swipe <x> <y> <x> <y> [s]
//                         a finger that moves from the first point to the
//                         second in the time, 0.3 s when none is given, and
//                         lifts there while it still moves, like a flick
//    drag <x> <y> <x> <y> [s]
//                         a finger held on the first point for the time, 0.6 s
//                         when none is given, that moves to the second point
//                         and rests there before it lifts, like a row moved
//    pinch <scale>        2 fingers at the middle of the screen, above 1 apart
//                         and below 1 together
//    screen               ok <width> <height>, the screen in points
//    dump [system]        ok <tree>, what the app shows, the keyboard included,
//                         or with `system` what the system shows over it, like
//                         an alert. A line end is `\n` and a backslash `\\`
//    batch <request>; <request>; ...
//                         the requests in a row with no wait for the app to
//                         stand still between them, so a swipe can land on a
//                         view that still moves. `wait <s>` is a pause
//    type <text>          taps the keys of the screen keyboard, one per character
//    key return|delete    one named key of the screen keyboard
//    keyboard <0|1>       waits until the keyboard is down or up
//    has key <name>       ok 1 when the keyboard shows that key
//    secure               ok <n>, the count of secure system text fields
//    value                ok <text>, what the system text field holds
//    ink <x> <y> <w> <h>  ok <left> <top> <right> <bottom> <r> <g> <b>, the box and
//                         the darkest color of what is drawn over the background
//

import Darwin
import ObjectiveC
import XCTest

/// XCTest waits until the app on screen stands still before every gesture,
/// so 2 swipes never meet a moving view. This switch skips that wait during a
/// `batch`, the way WebDriverAgent does it: the wait method of the private
/// `XCUIApplicationProcess` gets a body that asks the switch first.
private enum IdleWait {
    static var on = true

    private typealias Wait = @convention(c) (AnyObject, Selector, Bool) -> Void
    private typealias WaitPre = @convention(c) (AnyObject, Selector, Bool, Bool) -> Void

    static let installed: Bool = {
        guard let process = NSClassFromString("XCUIApplicationProcess") else { return false }
        var done = false
        let one = NSSelectorFromString("waitForQuiescenceIncludingAnimationsIdle:")
        if let method = class_getInstanceMethod(process, one) {
            let original = unsafeBitCast(method_getImplementation(method), to: Wait.self)
            let body: @convention(block) (AnyObject, Bool) -> Void = { this, animations in
                if IdleWait.on { original(this, one, animations) }
            }
            method_setImplementation(method, imp_implementationWithBlock(body))
            done = true
        }
        let two = NSSelectorFromString("waitForQuiescenceIncludingAnimationsIdle:isPreEvent:")
        if let method = class_getInstanceMethod(process, two) {
            let original = unsafeBitCast(method_getImplementation(method), to: WaitPre.self)
            let body: @convention(block) (AnyObject, Bool, Bool) -> Void = { this, animations, pre in
                if IdleWait.on { original(this, two, animations, pre) }
            }
            method_setImplementation(method, imp_implementationWithBlock(body))
            done = true
        }
        return done
    }()
}

final class SystemInput: XCTestCase {
    private var app: XCUIApplication!

    func testServe() throws {
        let environment = ProcessInfo.processInfo.environment
        let bundle = try XCTUnwrap(environment["HILEN_BUNDLE_ID"], "HILEN_BUNDLE_ID is not set")
        let port = UInt16(environment["HILEN_SYSTEM_INPUT_PORT"] ?? "") ?? 47815

        app = XCUIApplication(bundleIdentifier: bundle)

        let listener = try listen(port: port)
        defer { close(listener) }

        // One engine run is one connection. The lane stops this test when the
        // app is gone, so the loop has no end of its own.
        while true {
            let client = accept(listener, nil, nil)
            if client < 0 { continue }
            serve(client)
            close(client)
        }
    }

    private func listen(port: UInt16) throws -> Int32 {
        let listener = socket(AF_INET, SOCK_STREAM, 0)
        var yes: Int32 = 1
        setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))

        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = port.bigEndian
        address.sin_addr.s_addr = inet_addr("127.0.0.1")

        let bound = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(listener, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        XCTAssertEqual(bound, 0, "cannot bind port \(port)")
        XCTAssertEqual(Darwin.listen(listener, 1), 0)
        return listener
    }

    private func serve(_ client: Int32) {
        var pending = [UInt8]()
        var chunk = [UInt8](repeating: 0, count: 4096)

        while true {
            let count = read(client, &chunk, chunk.count)
            if count <= 0 { return }
            pending.append(contentsOf: chunk[0..<count])

            while let end = pending.firstIndex(of: 10) {
                let line = String(decoding: pending[0..<end], as: UTF8.self)
                pending.removeSubrange(0...end)

                let reply: String
                do {
                    reply = "ok " + (try handle(line))
                } catch {
                    reply = "error \(error)"
                }
                let bytes = Array((reply + "\n").utf8)
                _ = write(client, bytes, bytes.count)
            }
        }
    }

    private struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    private func handle(_ line: String) throws -> String {
        let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
        let command = parts.first ?? ""
        let rest = parts.count > 1 ? parts[1] : ""

        switch command {
        case "batch":
            if !IdleWait.installed {
                throw Failure(description: "this XCTest has no wait method to skip, a batch would wait like single requests")
            }
            IdleWait.on = false
            defer { IdleWait.on = true }
            var replies: [String] = []
            for step in rest.split(separator: ";").map({ $0.trimmingCharacters(in: .whitespaces) }) where !step.isEmpty {
                replies.append(try handle(step))
            }
            return replies.filter { !$0.isEmpty }.joined(separator: " ")
        case "wait":
            let seconds = try floats(rest, count: 1)[0]
            Thread.sleep(forTimeInterval: seconds)
            return ""
        case "tap":
            let numbers = try floats(rest, count: 2)
            point(numbers[0], numbers[1]).tap()
            return ""
        case "double":
            let numbers = try floats(rest, count: 2)
            point(numbers[0], numbers[1]).doubleTap()
            return ""
        case "hold":
            let numbers = try floats(rest, counts: [2, 3])
            point(numbers[0], numbers[1]).press(forDuration: numbers.count == 3 ? numbers[2] : 1)
            return ""
        case "swipe":
            let numbers = try floats(rest, counts: [4, 5])
            let seconds = numbers.count == 5 ? numbers[4] : 0.3
            try move(numbers, hold: 0.05, seconds: seconds, rest: 0)
            return ""
        case "drag":
            let numbers = try floats(rest, counts: [4, 5])
            let hold = numbers.count == 5 ? numbers[4] : 0.6
            try move(numbers, hold: hold, seconds: 0.5, rest: 0.3)
            return ""
        case "pinch":
            let scale = try floats(rest, count: 1)[0]
            if scale <= 0 || scale == 1 {
                throw Failure(description: "a pinch needs a scale above 0 that is not 1, got \(scale)")
            }
            // The velocity is in scale per second, its sign says apart or
            // together, so the pinch takes about half a second.
            try running().pinch(withScale: scale, velocity: (scale - 1) / 0.5)
            return ""
        case "screen":
            let frame = springboard.frame
            return "\(frame.width) \(frame.height)"
        case "dump":
            switch rest {
            case "":
                return oneLine(try tree(running()))
            case "system":
                return oneLine(try tree(springboard))
            default:
                throw Failure(description: "dump takes nothing or system, got '\(rest)'")
            }
        case "type":
            for character in rest {
                try tapKey(character)
            }
            return ""
        case "key":
            try tapNamedKey(rest)
            return ""
        case "keyboard":
            let up = rest == "1"
            let keyboard = app.keyboards.firstMatch
            let done = up
                ? keyboard.waitForExistence(timeout: 10) && waitOnScreen(keyboard, timeout: 10)
                : waitUntilDown(keyboard, timeout: 10)
            if !done {
                // A read of the frame of an element that is not there ends the
                // whole test, and with it the helper.
                let place = keyboard.exists ? "it is at \(keyboard.frame)" : "there is none"
                throw Failure(
                    description: "the keyboard is not \(up ? "up" : "down") after 10 s, \(place) on a screen of \(springboard.frame)"
                )
            }
            return ""
        case "has":
            let name = rest.replacingOccurrences(of: "key ", with: "")
            return app.keyboards.keys[name].exists ? "1" : "0"
        case "secure":
            return String(app.secureTextFields.count)
        case "value":
            return try systemFieldValue()
        case "ink":
            let numbers = try floats(rest, count: 4)
            return try ink(CGRect(x: numbers[0], y: numbers[1], width: numbers[2], height: numbers[3]))
        default:
            throw Failure(description: "unknown request '\(line)'")
        }
    }

    private func floats(_ text: String, count: Int) throws -> [CGFloat] {
        let numbers = text.split(separator: " ").compactMap { Double($0) }.map { CGFloat($0) }
        if numbers.count != count {
            throw Failure(description: "expected \(count) numbers, got '\(text)'")
        }
        return numbers
    }

    private func floats(_ text: String, counts: [Int]) throws -> [CGFloat] {
        let numbers = text.split(separator: " ").compactMap { Double($0) }.map { CGFloat($0) }
        if !counts.contains(numbers.count) {
            let wanted = counts.map(String.init).joined(separator: " or ")
            throw Failure(description: "expected \(wanted) numbers, got '\(text)'")
        }
        return numbers
    }

    /// A point of the screen. The app holds it while it runs, the home screen
    /// when the app is gone, so a tap after a crash does not end the helper.
    private func point(_ x: CGFloat, _ y: CGFloat) -> XCUICoordinate {
        let holder = app.state == .runningForeground ? app! : springboard
        return holder.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    private func running() throws -> XCUIApplication {
        if app.state != .runningForeground {
            throw Failure(description: "the app is not in front")
        }
        return app
    }

    /// What `element` shows, 1 element per line, the children of an element
    /// indented below it, with the frame in screen points.
    private func tree(_ element: XCUIElement) throws -> String {
        var lines: [String] = []
        add(try element.snapshot(), depth: 0, to: &lines)
        return lines.joined(separator: "\n")
    }

    private func add(_ node: XCUIElementSnapshot, depth: Int, to lines: inout [String]) {
        var line = String(repeating: "  ", count: depth) + typeName(node.elementType)
        if !node.label.isEmpty {
            line += " '\(node.label)'"
        }
        if !node.identifier.isEmpty {
            line += " id=\(node.identifier)"
        }
        if let value = node.value as? String, !value.isEmpty {
            line += " value='\(value)'"
        }
        if !node.isEnabled {
            line += " disabled"
        }
        if node.isSelected {
            line += " selected"
        }
        let frame = node.frame
        line += " [\(frame.minX), \(frame.minY)] \(frame.width)x\(frame.height)"
        lines.append(line)
        for child in node.children {
            add(child, depth: depth + 1, to: &lines)
        }
    }

    private func typeName(_ type: XCUIElement.ElementType) -> String {
        switch type {
        case .application: return "Application"
        case .window: return "Window"
        case .other: return "Other"
        case .alert: return "Alert"
        case .sheet: return "Sheet"
        case .button: return "Button"
        case .keyboard: return "Keyboard"
        case .key: return "Key"
        case .textField: return "TextField"
        case .secureTextField: return "SecureTextField"
        case .textView: return "TextView"
        case .staticText: return "StaticText"
        case .image: return "Image"
        case .icon: return "Icon"
        case .cell: return "Cell"
        case .table: return "Table"
        case .collectionView: return "CollectionView"
        case .scrollView: return "ScrollView"
        case .navigationBar: return "NavigationBar"
        case .statusBar: return "StatusBar"
        case .switch: return "Switch"
        case .toolbar: return "Toolbar"
        case .menu: return "Menu"
        case .menuItem: return "MenuItem"
        case .pickerWheel: return "PickerWheel"
        case .webView: return "WebView"
        case .group: return "Group"
        default: return "Element\(type.rawValue)"
        }
    }

    /// The home screen, which holds the alerts and sheets of the system and
    /// is there whether the app runs or not.
    private var springboard: XCUIApplication {
        XCUIApplication(bundleIdentifier: "com.apple.springboard")
    }

    /// A finger on the first 2 of `numbers` for `hold` seconds, then a move
    /// to the next 2 in `seconds`, then `rest` seconds there before it lifts.
    private func move(_ numbers: [CGFloat], hold: CGFloat, seconds: CGFloat, rest: CGFloat) throws {
        if seconds <= 0 {
            throw Failure(description: "a move needs a time above 0, got \(seconds)")
        }
        let distance = hypot(numbers[2] - numbers[0], numbers[3] - numbers[1])
        point(numbers[0], numbers[1]).press(
            forDuration: hold,
            thenDragTo: point(numbers[2], numbers[3]),
            withVelocity: XCUIGestureVelocity(rawValue: max(distance / seconds, 1)),
            thenHoldForDuration: rest
        )
    }

    /// A tap of an element that is not there ends the whole test, and with it
    /// the helper, so it is checked first.
    private func tapIfThere(_ element: XCUIElement, _ missing: String) throws {
        if !element.exists {
            throw Failure(description: missing)
        }
        element.tap()
    }

    private func oneLine(_ text: String) -> String {
        text.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\n", with: "\\n")
    }

    /// A keyboard that went down can stay in the tree below the screen, it
    /// counts as down then, seen on 2026-11-07.
    private func waitUntilDown(_ keyboard: XCUIElement, timeout: TimeInterval) -> Bool {
        let screen = springboard.frame
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            if !keyboard.exists || keyboard.frame.minY >= screen.maxY {
                return true
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
        return false
    }

    /// The keyboard is there from the moment it starts to slide in, while its
    /// keys are still below the screen and a tap on one lands on nothing, seen
    /// on 2026-11-07. This waits until its top is on the screen and still.
    private func waitOnScreen(_ keyboard: XCUIElement, timeout: TimeInterval) -> Bool {
        let screen = springboard.frame
        var last = keyboard.frame.minY
        let end = Date().addingTimeInterval(timeout)
        while Date() < end {
            Thread.sleep(forTimeInterval: 0.1)
            if !keyboard.exists {
                return false
            }
            let top = keyboard.frame.minY
            if top < screen.maxY && top == last {
                return true
            }
            last = top
        }
        return false
    }

    private func tapNamedKey(_ name: String) throws {
        let keyboard = app.keyboards.firstMatch
        // The Return key is a button and its title follows the field, Return,
        // Done, Go and so on, so it is found by its place, not by its name.
        let candidates: [XCUIElement]
        switch name {
        case "return":
            candidates = ["Return", "return", "Done", "done", "Go", "go"].map { keyboard.buttons[$0] }
        case "delete":
            candidates = [keyboard.keys["delete"], keyboard.keys["Delete"]]
        default:
            throw Failure(description: "unknown key '\(name)'")
        }
        guard let key = candidates.first(where: { $0.exists }) else {
            throw Failure(description: "the keyboard has no \(name) key")
        }
        key.tap()
    }

    /// One character through the keys a person would tap: the key itself, the
    /// key behind shift, or the key on the numbers page.
    private func tapKey(_ character: Character) throws {
        let keyboard = app.keyboards.firstMatch
        if !keyboard.waitForExistence(timeout: 10) {
            throw Failure(description: "no keyboard to type '\(character)' on")
        }
        if !waitOnScreen(keyboard, timeout: 10) {
            throw Failure(description: "the keyboard did not come onto the screen to type '\(character)'")
        }

        if character == " " {
            try tapIfThere(keyboard.keys["space"], "the keyboard has no space key")
            return
        }

        let name = String(character)
        if keyboard.keys[name].exists {
            keyboard.keys[name].tap()
            return
        }

        // Shift goes from off to on to caps lock and back to off. A test before
        // could leave the keyboard in caps lock, and 1 tap then gave capitals
        // again, seen on 2026-11-07.
        if character.isLetter {
            for _ in 0..<3 {
                try tapIfThere(keyboard.buttons["shift"], "the keyboard has no shift key to type '\(name)'")
                if keyboard.keys[name].exists {
                    keyboard.keys[name].tap()
                    return
                }
            }
            throw Failure(description: "the keyboard has no '\(name)' key, also not behind shift")
        }

        // A test before can leave the keyboard on another page, so 1 tap of
        // more can lead away from the digits, seen on 2026-11-07.
        var found = false
        for _ in 0..<2 {
            let more = keyboard.keys["more"]
            if !more.exists {
                throw Failure(description: "the keyboard has no '\(name)' key and no second page")
            }
            more.tap()
            if keyboard.keys[name].exists {
                found = true
                break
            }
        }
        guard found else {
            throw Failure(description: "the keyboard has no '\(name)' key, also not on the second page")
        }
        keyboard.keys[name].tap()
        // A space or a letter flips the page back by itself, a digit does not.
        if keyboard.keys["more"].exists && !keyboard.keys["q"].exists && !keyboard.keys["Q"].exists {
            try tapIfThere(keyboard.keys["more"], "the keyboard lost its more key")
        }
    }

    private func systemFieldValue() throws -> String {
        for query in [app.textFields, app.secureTextFields, app.textViews] {
            let field = query.firstMatch
            if field.exists {
                return (field.value as? String ?? "").replacingOccurrences(of: "\n", with: "\\n")
            }
        }
        throw Failure(description: "no system text field on screen")
    }

    /// What is drawn inside `area` over its background. The background is the
    /// color of the top left pixel of the area. Every pixel far enough from it
    /// counts as ink.
    private func ink(_ area: CGRect) throws -> String {
        let shot = XCUIScreen.main.screenshot().image
        guard let image = shot.cgImage else {
            throw Failure(description: "no screenshot")
        }
        let scale = CGFloat(image.width) / shot.size.width
        let x0 = Int(area.minX * scale), y0 = Int(area.minY * scale)
        let width = Int(area.width * scale), height = Int(area.height * scale)

        guard x0 >= 0, y0 >= 0, x0 + width <= image.width, y0 + height <= image.height, width > 0, height > 0
        else {
            throw Failure(description: "the area \(area) is outside the screen")
        }

        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let context = CGContext(
            data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        // `cropping` counts from the top left, and the first row in the memory
        // of a bitmap context is the top row of what was drawn.
        guard let cropped = image.cropping(to: CGRect(x: x0, y: y0, width: width, height: height)) else {
            throw Failure(description: "cannot crop the screenshot to \(area)")
        }
        context.draw(cropped, in: CGRect(x: 0, y: 0, width: width, height: height))

        func color(_ x: Int, _ y: Int) -> (Int, Int, Int) {
            let index = (y * width + x) * 4
            return (Int(pixels[index]), Int(pixels[index + 1]), Int(pixels[index + 2]))
        }

        let background = color(0, 0)
        var left = width, top = height, right = -1, bottom = -1
        var darkest = background
        var farthest = 0

        for y in 0..<height {
            for x in 0..<width {
                let pixel = color(x, y)
                let distance =
                    abs(pixel.0 - background.0) + abs(pixel.1 - background.1) + abs(pixel.2 - background.2)
                if distance < 96 { continue }
                left = min(left, x)
                top = min(top, y)
                right = max(right, x)
                bottom = max(bottom, y)
                if distance > farthest {
                    farthest = distance
                    darkest = pixel
                }
            }
        }

        if right < 0 {
            throw Failure(description: "nothing is drawn in \(area)")
        }
        return "\(left) \(top) \(right) \(bottom) \(darkest.0) \(darkest.1) \(darkest.2)"
    }
}
