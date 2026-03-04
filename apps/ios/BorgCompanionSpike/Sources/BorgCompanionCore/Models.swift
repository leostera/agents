import Foundation

public enum BorgCompanionEventKind: String, Codable, Equatable {
    case responseAvailable = "response_available"
    case error
    case runtimeStatus = "runtime_status"
}

public struct BorgCompanionEvent: Codable, Equatable {
    public var version: Int
    public var eventId: String
    public var sessionId: String?
    public var kind: BorgCompanionEventKind
    public var title: String
    public var body: String
    public var createdAt: Date
    public var requiresOpen: Bool

    public init(
        version: Int,
        eventId: String,
        sessionId: String?,
        kind: BorgCompanionEventKind,
        title: String,
        body: String,
        createdAt: Date,
        requiresOpen: Bool
    ) {
        self.version = version
        self.eventId = eventId
        self.sessionId = sessionId
        self.kind = kind
        self.title = title
        self.body = body
        self.createdAt = createdAt
        self.requiresOpen = requiresOpen
    }
}

public struct BorgCompanionPairingChallenge: Codable, Equatable {
    public var pairingCode: String
    public var nonce: String
    public var expiresAt: Date

    public init(pairingCode: String, nonce: String, expiresAt: Date) {
        self.pairingCode = pairingCode
        self.nonce = nonce
        self.expiresAt = expiresAt
    }
}

public struct BorgCompanionDeviceRegistration: Codable, Equatable {
    public var deviceId: String
    public var pushToken: String
    public var publicKey: String
    public var platform: String

    public init(deviceId: String, pushToken: String, publicKey: String, platform: String = "ios") {
        self.deviceId = deviceId
        self.pushToken = pushToken
        self.publicKey = publicKey
        self.platform = platform
    }
}
