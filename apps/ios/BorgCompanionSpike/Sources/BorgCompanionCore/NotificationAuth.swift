import Foundation

#if canImport(UserNotifications)
import UserNotifications

public protocol BorgCompanionNotificationAuthorizing {
    func requestAuthorization() async throws -> Bool
    func currentAuthorizationStatus() async -> UNAuthorizationStatus
}

public struct BorgCompanionNotificationAuthClient: BorgCompanionNotificationAuthorizing {
    public init() {}

    public func requestAuthorization() async throws -> Bool {
        try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
    }

    public func currentAuthorizationStatus() async -> UNAuthorizationStatus {
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().getNotificationSettings { settings in
                continuation.resume(returning: settings.authorizationStatus)
            }
        }
    }
}
#endif
