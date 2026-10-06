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
//  Requests, all positions in screen points:
//
//    tap <x> <y>          a tap on the screen
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
import XCTest

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
        case "tap":
            let numbers = try floats(rest, count: 2)
            point(numbers[0], numbers[1]).tap()
            return ""
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
                ? keyboard.waitForExistence(timeout: 10)
                : waitUntilGone(keyboard, timeout: 10)
            if !done {
                throw Failure(description: "the keyboard is not \(up ? "up" : "down") after 10 s")
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

    private func point(_ x: CGFloat, _ y: CGFloat) -> XCUICoordinate {
        app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    private func waitUntilGone(_ element: XCUIElement, timeout: TimeInterval) -> Bool {
        let gone = NSPredicate(format: "exists == false")
        let waiting = XCTNSPredicateExpectation(predicate: gone, object: element)
        return XCTWaiter().wait(for: [waiting], timeout: timeout) == .completed
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

        if character == " " {
            keyboard.keys["space"].tap()
            return
        }

        let name = String(character)
        if keyboard.keys[name].exists {
            keyboard.keys[name].tap()
            return
        }

        if character.isLetter {
            keyboard.buttons["shift"].tap()
            if keyboard.keys[name].exists {
                keyboard.keys[name].tap()
                return
            }
            throw Failure(description: "the keyboard has no '\(name)' key, also not behind shift")
        }

        let more = keyboard.keys["more"]
        if !more.exists {
            throw Failure(description: "the keyboard has no '\(name)' key and no second page")
        }
        more.tap()
        guard keyboard.keys[name].exists else {
            throw Failure(description: "the keyboard has no '\(name)' key, also not on the second page")
        }
        keyboard.keys[name].tap()
        // A space or a letter flips the page back by itself, a digit does not.
        if keyboard.keys["more"].exists && !keyboard.keys["q"].exists && !keyboard.keys["Q"].exists {
            keyboard.keys["more"].tap()
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
