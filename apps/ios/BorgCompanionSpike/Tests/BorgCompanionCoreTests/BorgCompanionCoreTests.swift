import BorgCompanionCore
import Foundation
import Testing

private func decode(_ json: String) throws -> [AnyHashable: Any] {
    let data = json.data(using: .utf8)!
    let any = try JSONSerialization.jsonObject(with: data, options: [])
    guard let dictionary = any as? [String: Any] else {
        throw NSError(domain: "test", code: 1)
    }
    return dictionary.reduce(into: [AnyHashable: Any]()) { partial, item in
        partial[item.key] = item.value
    }
}

@Test
func decodesBorgPayloadWithApsFallback() throws {
    let payload = try decode(
        #"{"aps":{"alert":{"title":"Borg","body":"Response ready"}},"borg":{"version":1,"eventId":"evt_123","sessionId":"session_1","kind":"response_available","title":"","body":"","createdAt":"2026-03-04T12:00:00Z","requiresOpen":true}}"#
    )

    let decoder = BorgCompanionPushDecoder()
    let event = try decoder.decodeEvent(userInfo: payload)

    #expect(event.kind == .responseAvailable)
    #expect(event.title == "Borg")
    #expect(event.body == "Response ready")
    #expect(event.sessionId == "session_1")
    #expect(event.requiresOpen == true)
}

@Test
func detectsNonBorgPayload() throws {
    let payload = try decode(#"{"aps":{"alert":"hello"}}"#)
    let decoder = BorgCompanionPushDecoder()

    #expect(decoder.isBorgEventPayload(userInfo: payload) == false)
}

@Test
func decodeFailsWithoutBorgRoot() throws {
    let payload = try decode(#"{"aps":{"alert":"hello"}}"#)
    let decoder = BorgCompanionPushDecoder()

    #expect(throws: BorgCompanionPushDecoderError.self) {
        try decoder.decodeEvent(userInfo: payload)
    }
}
