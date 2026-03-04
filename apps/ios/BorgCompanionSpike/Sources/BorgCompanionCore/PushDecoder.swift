import Foundation

public enum BorgCompanionPushDecoderError: Error {
    case invalidUserInfo
    case invalidBorgPayload
}

public struct BorgCompanionPushDecoder {
    public init() {}

    public func decodeEvent(userInfo: [AnyHashable: Any]) throws -> BorgCompanionEvent {
        let root = try jsonObject(from: userInfo)
        guard let rootDict = root as? [String: Any] else {
            throw BorgCompanionPushDecoderError.invalidUserInfo
        }

        guard let borgAny = rootDict["borg"] else {
            throw BorgCompanionPushDecoderError.invalidBorgPayload
        }

        let borgData = try JSONSerialization.data(withJSONObject: borgAny, options: [])
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        var event = try decoder.decode(BorgCompanionEvent.self, from: borgData)

        if event.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            event.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            let (alertTitle, alertBody) = extractApsAlert(rootDict)
            if event.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                event.title = alertTitle
            }
            if event.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                event.body = alertBody
            }
        }

        return event
    }

    public func isBorgEventPayload(userInfo: [AnyHashable: Any]) -> Bool {
        guard let root = try? jsonObject(from: userInfo) as? [String: Any] else {
            return false
        }
        return root["borg"] != nil
    }

    private func jsonObject(from userInfo: [AnyHashable: Any]) throws -> Any {
        var jsonRoot: [String: Any] = [:]
        for (key, value) in userInfo {
            guard let key = key as? String else {
                continue
            }
            jsonRoot[key] = sanitize(value)
        }
        if !JSONSerialization.isValidJSONObject(jsonRoot) {
            throw BorgCompanionPushDecoderError.invalidUserInfo
        }
        let data = try JSONSerialization.data(withJSONObject: jsonRoot, options: [])
        return try JSONSerialization.jsonObject(with: data, options: [])
    }

    private func sanitize(_ value: Any) -> Any {
        if let dict = value as? [AnyHashable: Any] {
            var out: [String: Any] = [:]
            for (key, nested) in dict {
                if let key = key as? String {
                    out[key] = sanitize(nested)
                }
            }
            return out
        }
        if let array = value as? [Any] {
            return array.map { sanitize($0) }
        }
        return value
    }

    private func extractApsAlert(_ root: [String: Any]) -> (String, String) {
        guard
            let aps = root["aps"] as? [String: Any],
            let alert = aps["alert"]
        else {
            return ("Borg", "")
        }

        if let alert = alert as? String {
            return ("Borg", alert)
        }

        if let alert = alert as? [String: Any] {
            let title = (alert["title"] as? String) ?? "Borg"
            let body = (alert["body"] as? String) ?? ""
            return (title, body)
        }

        return ("Borg", "")
    }
}
