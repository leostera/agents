import BorgCompanionCore
import Foundation

func fail(_ message: String) -> Never {
    fputs("error: \(message)\n", stderr)
    exit(1)
}

let args = CommandLine.arguments
if args.count < 2 {
    fail("usage: BorgCompanionPushFixture <payload-json-file>")
}

let path = args[1]
let url = URL(fileURLWithPath: path)
let data: Data

do {
    data = try Data(contentsOf: url)
} catch {
    fail("unable to read file: \(error.localizedDescription)")
}

let any: Any
do {
    any = try JSONSerialization.jsonObject(with: data, options: [])
} catch {
    fail("payload is not valid JSON: \(error.localizedDescription)")
}

guard let dictionary = any as? [String: Any] else {
    fail("payload root must be a JSON object")
}

let userInfo = dictionary.reduce(into: [AnyHashable: Any]()) { partial, item in
    partial[item.key] = item.value
}

let decoder = BorgCompanionPushDecoder()
if !decoder.isBorgEventPayload(userInfo: userInfo) {
    fail("payload does not contain `borg` root object")
}

let event: BorgCompanionEvent
do {
    event = try decoder.decodeEvent(userInfo: userInfo)
} catch {
    fail("decode failed: \(error)")
}

let encoder = JSONEncoder()
encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
encoder.dateEncodingStrategy = .iso8601

let output: Data
do {
    output = try encoder.encode(event)
} catch {
    fail("encode failed: \(error)")
}

if let text = String(data: output, encoding: .utf8) {
    print(text)
} else {
    fail("unable to render JSON as UTF-8")
}
