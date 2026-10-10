//
//  The hands of a UI test on an Apple TV: a press of a button of the remote.
//  The engine cannot send that to itself, a press comes from the system. So
//  the engine test connects here over a local socket and asks, one line per
//  request, and this side answers `ok` or `error` after the action is done.
//
//  Requests:
//
//    remote up|down|left|right|select|menu|playpause    one button of the remote
//

import Darwin
import XCTest

final class SystemInput: XCTestCase {
    func testServe() throws {
        let environment = ProcessInfo.processInfo.environment
        let port = UInt16(environment["HILEN_SYSTEM_INPUT_PORT"] ?? "") ?? 47815

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
        case "remote":
            XCUIRemote.shared.press(try button(rest))
            return ""
        default:
            throw Failure(description: "unknown request '\(line)'")
        }
    }

    private func button(_ name: String) throws -> XCUIRemote.Button {
        switch name {
        case "up": return .up
        case "down": return .down
        case "left": return .left
        case "right": return .right
        case "select": return .select
        case "menu": return .menu
        case "playpause": return .playPause
        default: throw Failure(description: "unknown button '\(name)'")
        }
    }
}
