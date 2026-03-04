import Foundation

public struct BorgCompanionSyncRequest: Codable, Equatable {
    public var deviceId: String
    public var afterEventId: String?
    public var limit: Int

    public init(deviceId: String, afterEventId: String?, limit: Int = 50) {
        self.deviceId = deviceId
        self.afterEventId = afterEventId
        self.limit = limit
    }
}

public struct BorgCompanionSyncResponse: Codable, Equatable {
    public var events: [BorgCompanionEvent]
    public var nextCursor: String?

    public init(events: [BorgCompanionEvent], nextCursor: String?) {
        self.events = events
        self.nextCursor = nextCursor
    }
}

public struct BorgCompanionPairingResponse: Codable, Equatable {
    public var deviceId: String
    public var sessionToken: String
    public var apiBaseURL: String

    public init(deviceId: String, sessionToken: String, apiBaseURL: String) {
        self.deviceId = deviceId
        self.sessionToken = sessionToken
        self.apiBaseURL = apiBaseURL
    }
}
