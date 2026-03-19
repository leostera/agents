import Foundation
import Speech

public typealias AgentsTranscriptCallback = @convention(c) (UnsafePointer<CChar>?, Int32) -> Void

private func emit(_ callbackPtr: UnsafeRawPointer, text: String, isFinal: Bool) {
    let callback = unsafeBitCast(callbackPtr, to: AgentsTranscriptCallback.self)
    text.withCString { cString in
        callback(cString, isFinal ? 1 : 0)
    }
}

private func emitDebug(_ callbackPtr: UnsafeRawPointer, text: String) {
    let callback = unsafeBitCast(callbackPtr, to: AgentsTranscriptCallback.self)
    text.withCString { cString in
        callback(cString, -1)
    }
}

private func waitSpeechAuthorization() -> Bool {
    let sem = DispatchSemaphore(value: 0)
    var allowed = false
    SFSpeechRecognizer.requestAuthorization { status in
        allowed = (status == .authorized)
        sem.signal()
    }
    _ = sem.wait(timeout: .now() + .seconds(10))
    return allowed
}

@_cdecl("agents_apple_transcribe_file")
public func agents_apple_transcribe_file(
    _ pathPtr: UnsafePointer<CChar>?,
    _ localePtr: UnsafePointer<CChar>?,
    _ callbackPtr: UnsafeRawPointer?
) -> Int32 {
    guard let pathPtr, let callbackPtr else {
        return 5
    }

    guard waitSpeechAuthorization() else {
        emitDebug(callbackPtr, text: "speech authorization denied")
        return 2
    }

    let recognizer: SFSpeechRecognizer?
    if let localePtr {
        recognizer = SFSpeechRecognizer(locale: Locale(identifier: String(cString: localePtr)))
    } else {
        recognizer = SFSpeechRecognizer()
    }

    guard let recognizer, recognizer.isAvailable else {
        emitDebug(callbackPtr, text: "speech recognizer unavailable")
        return 1
    }

    let callbackQueue = OperationQueue()
    callbackQueue.name = "agents.llm.apple.transcription"
    callbackQueue.maxConcurrentOperationCount = 1
    recognizer.queue = callbackQueue

    let path = String(cString: pathPtr)
    let url = URL(fileURLWithPath: path)
    let request = SFSpeechURLRecognitionRequest(url: url)
    request.shouldReportPartialResults = false

    let sem = DispatchSemaphore(value: 0)
    var sawResult = false

    let task = recognizer.recognitionTask(with: request) { result, error in
        if let result {
            sawResult = true
            emit(
                callbackPtr,
                text: result.bestTranscription.formattedString,
                isFinal: result.isFinal
            )
            if result.isFinal {
                sem.signal()
            }
        }
        if let error {
            emitDebug(callbackPtr, text: "recognition error: \(error.localizedDescription)")
            sem.signal()
        }
    }

    let startedAt = Date()
    while true {
        if sem.wait(timeout: .now() + .milliseconds(50)) == .success {
            break
        }
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.001))
        if Date().timeIntervalSince(startedAt) >= 30 {
            task.cancel()
            emitDebug(callbackPtr, text: "recognition timeout")
            return 6
        }
    }

    if !sawResult {
        return 4
    }

    return 0
}
