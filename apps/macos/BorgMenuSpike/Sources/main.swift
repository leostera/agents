import AppKit
import AVFoundation
import Darwin
import Foundation
import Speech

private enum WakeState {
    case waiting
    case recording
    case error
}

@MainActor
final class SpikeAppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private let menu = NSMenu()
    private let statusMenuItem = NSMenuItem(title: "Status: booting", action: nil, keyEquivalent: "")
    private let transcriptMenuItem = NSMenuItem(title: "Last transcript: (none)", action: nil, keyEquivalent: "")
    private let serviceMenuItem = NSMenuItem(title: "Borg service: stopped", action: nil, keyEquivalent: "")
    private let startServiceMenuItem = NSMenuItem(title: "Start Borg", action: nil, keyEquivalent: "s")
    private let stopServiceMenuItem = NSMenuItem(title: "Stop Borg", action: nil, keyEquivalent: "x")
    private let restartServiceMenuItem = NSMenuItem(title: "Restart Borg", action: nil, keyEquivalent: "r")
    private let loginItemsMenuItem = NSMenuItem(title: "Open Login Items Settings", action: nil, keyEquivalent: "l")

    private let audioEngine = AVAudioEngine()
    private let speechRecognizer = SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?

    private var state: WakeState = .waiting
    private var wakePhrase = "hey borg"
    private var normalizedWakePhrase = "hey borg"
    private var wakePhraseRegex: NSRegularExpression?
    private var debugTranscripts = false
    private var silenceSeconds: TimeInterval = 1.4
    private var lastSpeechAt: Date?
    private var latestUtterance = ""
    private var silenceTimer: Timer?
    private var recognitionSessionId: UInt64 = 0
    private var restartWorkItem: DispatchWorkItem?
    private var borgProcess: Process?
    private var borgPath: String?
    private var borgArguments: [String] = ["start"]
    private var autoStartBorg = false
    private var restartRequested = false
    private var lastObservedTranscript = ""

    func applicationDidFinishLaunching(_ notification: Notification) {
        configureFromEnvironment()
        setupStatusBar()
        print("borg_menu_spike:status_item_ready")
        fflush(stdout)

        if autoStartBorg {
            startBorgService()
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let speechOk = waitSpeechAuthorization()
            let micOk = speechOk ? waitMicrophoneAuthorization() : false

            Task { @MainActor [weak self] in
                guard let self else { return }
                guard speechOk else {
                    self.transitionToError("Speech recognition permission denied")
                    return
                }
                guard micOk else {
                    self.transitionToError("Microphone permission denied")
                    return
                }
                do {
                    try self.startRecognitionLoop()
                    self.transitionToWaiting("Waiting for \"\(self.wakePhrase)\"")
                } catch {
                    self.transitionToError("Failed starting recognizer: \(error.localizedDescription)")
                }
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        restartWorkItem?.cancel()
        restartWorkItem = nil
        stopRecognitionLoop()
        stopBorgService(forceKillAfterSeconds: 0)
    }

    private func configureFromEnvironment() {
        let env = ProcessInfo.processInfo.environment
        if let phrase = env["BORG_VOICEWAKE_PHRASE"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !phrase.isEmpty
        {
            wakePhrase = phrase.lowercased()
        }
        normalizedWakePhrase = normalizeWakeString(wakePhrase)
        wakePhraseRegex = buildWakePhraseRegex(normalizedWakePhrase)
        debugTranscripts = parseEnvBool(env["BORG_VOICEWAKE_DEBUG_TRANSCRIPTS"])
        if let silenceRaw = env["BORG_VOICEWAKE_SILENCE_SECONDS"],
           let parsed = Double(silenceRaw),
           parsed >= 0.5
        {
            silenceSeconds = parsed
        }

        borgPath = env["BORG_CLI_PATH"]
        if let argsRaw = env["BORG_CLI_ARGS"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !argsRaw.isEmpty
        {
            borgArguments = argsRaw.split(separator: " ").map(String.init)
        }
        autoStartBorg = parseEnvBool(env["BORG_MENU_SPIKE_AUTO_START_BORG"])
    }

    private func setupStatusBar() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        setStatusBubble(color: .systemGray, text: "Borg")

        statusMenuItem.isEnabled = false
        transcriptMenuItem.isEnabled = false
        serviceMenuItem.isEnabled = false

        startServiceMenuItem.target = self
        startServiceMenuItem.action = #selector(startBorgServiceAction)
        stopServiceMenuItem.target = self
        stopServiceMenuItem.action = #selector(stopBorgServiceAction)
        restartServiceMenuItem.target = self
        restartServiceMenuItem.action = #selector(restartBorgServiceAction)
        loginItemsMenuItem.target = self
        loginItemsMenuItem.action = #selector(openLoginItemsSettings)

        menu.addItem(statusMenuItem)
        menu.addItem(transcriptMenuItem)
        menu.addItem(.separator())
        menu.addItem(serviceMenuItem)
        menu.addItem(startServiceMenuItem)
        menu.addItem(stopServiceMenuItem)
        menu.addItem(restartServiceMenuItem)
        menu.addItem(loginItemsMenuItem)
        menu.addItem(.separator())

        let dashboard = NSMenuItem(title: "Open Dashboard", action: #selector(openDashboard), keyEquivalent: "d")
        dashboard.target = self
        menu.addItem(dashboard)

        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        statusItem?.menu = menu
        refreshServiceMenuState()
    }

    private func startRecognitionLoop() throws {
        guard let speechRecognizer else {
            throw NSError(domain: "borg.voicespike", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "Speech recognizer unavailable for locale",
            ])
        }
        guard speechRecognizer.isAvailable else {
            throw NSError(domain: "borg.voicespike", code: 2, userInfo: [
                NSLocalizedDescriptionKey: "Speech recognizer currently unavailable",
            ])
        }

        stopRecognitionLoop()
        recognitionSessionId += 1
        let sessionId = recognitionSessionId
        lastObservedTranscript = ""

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        if #available(macOS 13.0, *) {
            request.addsPunctuation = false
        }
        recognitionRequest = request

        recognitionTask = speechRecognizer.recognitionTask(with: request) { [weak self] result, error in
            DispatchQueue.main.async {
                self?.handleRecognitionResult(
                    sessionId: sessionId,
                    result: result,
                    error: error
                )
            }
        }

        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        inputNode.removeTap(onBus: 0)
        inputNode.installTap(
            onBus: 0,
            bufferSize: 2048,
            format: format,
            block: Self.makeTapHandler(request: request)
        )

        audioEngine.prepare()
        try audioEngine.start()

        silenceTimer?.invalidate()
        silenceTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.checkSilenceTimeout()
            }
        }
    }

    private func stopRecognitionLoop() {
        restartWorkItem?.cancel()
        restartWorkItem = nil
        silenceTimer?.invalidate()
        silenceTimer = nil

        audioEngine.stop()
        audioEngine.inputNode.removeTap(onBus: 0)

        recognitionRequest?.endAudio()
        recognitionRequest = nil

        recognitionTask?.cancel()
        recognitionTask = nil
    }

    private func handleRecognitionResult(
        sessionId: UInt64,
        result: SFSpeechRecognitionResult?,
        error: Error?
    ) {
        if sessionId != recognitionSessionId {
            return
        }

        if let error {
            if isCancellationError(error) {
                return
            }
            if isNoSpeechError(error) {
                scheduleRecognizerRestart(
                    afterSeconds: 0.3,
                    showError: false,
                    message: "No speech detected"
                )
                return
            }
            transitionToError("Speech recognition error: \(error.localizedDescription)")
            scheduleRecognizerRestart(
                afterSeconds: 1.0,
                showError: false,
                message: "Recovering recognizer..."
            )
            return
        }
        guard let result else { return }

        let transcript = result.bestTranscription.formattedString
        if debugTranscripts && transcript != lastObservedTranscript {
            lastObservedTranscript = transcript
            print("borg_menu_spike:transcript=\(transcript)")
            fflush(stdout)
        }

        switch state {
        case .waiting:
            if let wakeParts = wakePhraseParts(in: transcript) {
                latestUtterance = wakeParts.suffix
                lastSpeechAt = Date()
                logWakeParts(prefix: wakeParts.prefix, between: wakeParts.between, suffix: wakeParts.suffix)
                transitionToRecording()
                if !latestUtterance.isEmpty {
                    print("borg_menu_spike:recording_buffer=\(latestUtterance)")
                    fflush(stdout)
                }
            }
        case .recording:
            let utterance = extractPostWakeText(from: transcript)
            if !utterance.isEmpty {
                latestUtterance = utterance
                lastSpeechAt = Date()
            }
            if result.isFinal {
                finishRecording(reason: "final_result")
            }
        case .error:
            break
        }
    }

    private nonisolated static func makeTapHandler(
        request: SFSpeechAudioBufferRecognitionRequest
    ) -> (AVAudioPCMBuffer, AVAudioTime) -> Void {
        return { buffer, _ in
            request.append(buffer)
        }
    }

    private func extractPostWakeText(from transcript: String) -> String {
        guard let wakeParts = wakePhraseParts(in: transcript) else {
            return transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return wakeParts.suffix
    }

    private func checkSilenceTimeout() {
        guard state == .recording else { return }
        guard let lastSpeechAt else { return }
        if Date().timeIntervalSince(lastSpeechAt) >= silenceSeconds {
            finishRecording(reason: "silence_timeout")
        }
    }

    private func finishRecording(reason: String) {
        let spoken = latestUtterance.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalText = spoken.isEmpty ? "(no speech captured after wake phrase)" : spoken

        transcriptMenuItem.title = "Last transcript: \(finalText)"
        print("borg_menu_spike:transcription_final[\(reason)]=\(finalText)")
        fflush(stdout)

        latestUtterance = ""
        lastSpeechAt = nil
        scheduleRecognizerRestart(
            afterSeconds: 0.0,
            showError: false,
            message: "Waiting for \"\(wakePhrase)\""
        )
    }

    private func scheduleRecognizerRestart(
        afterSeconds: Double,
        showError: Bool,
        message: String
    ) {
        restartWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            do {
                try self.startRecognitionLoop()
                if showError {
                    self.transitionToError(message)
                } else {
                    self.transitionToWaiting(message)
                }
            } catch {
                self.transitionToError("Restart failed: \(error.localizedDescription)")
            }
        }
        restartWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + afterSeconds, execute: work)
    }

    private func refreshServiceMenuState() {
        if let process = borgProcess, process.isRunning {
            serviceMenuItem.title = "Borg service: running (pid \(process.processIdentifier))"
            startServiceMenuItem.isEnabled = false
            stopServiceMenuItem.isEnabled = true
            restartServiceMenuItem.isEnabled = true
        } else {
            if !serviceMenuItem.title.contains("error") && !serviceMenuItem.title.contains("stopped") {
                serviceMenuItem.title = "Borg service: stopped"
            }
            startServiceMenuItem.isEnabled = true
            stopServiceMenuItem.isEnabled = false
            restartServiceMenuItem.isEnabled = false
        }
    }

    @objc private func startBorgServiceAction() {
        startBorgService()
    }

    @objc private func stopBorgServiceAction() {
        restartRequested = false
        stopBorgService(forceKillAfterSeconds: 3)
    }

    @objc private func restartBorgServiceAction() {
        restartRequested = true
        if let process = borgProcess, process.isRunning {
            stopBorgService(forceKillAfterSeconds: 3)
        } else {
            startBorgService()
            restartRequested = false
        }
    }

    private func startBorgService() {
        if let process = borgProcess, process.isRunning {
            refreshServiceMenuState()
            return
        }

        let path = borgPath ?? resolveBorgBinaryPath()
        guard let path else {
            serviceMenuItem.title = "Borg service: error (set BORG_CLI_PATH)"
            refreshServiceMenuState()
            print("borg_menu_spike:borg_error=missing_borg_binary")
            fflush(stdout)
            return
        }
        borgPath = path

        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = borgArguments

        let outPipe = Pipe()
        let errPipe = Pipe()
        process.standardOutput = outPipe
        process.standardError = errPipe

        outPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                return
            }
            if let text = String(data: data, encoding: .utf8) {
                for line in text.split(whereSeparator: \.isNewline) {
                    print("borg_menu_spike:borg_stdout=\(line)")
                }
                fflush(stdout)
            }
        }
        errPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                return
            }
            if let text = String(data: data, encoding: .utf8) {
                for line in text.split(whereSeparator: \.isNewline) {
                    print("borg_menu_spike:borg_stderr=\(line)")
                }
                fflush(stdout)
            }
        }

        process.terminationHandler = { [weak self] proc in
            DispatchQueue.main.async {
                guard let self else { return }
                self.borgProcess = nil
                outPipe.fileHandleForReading.readabilityHandler = nil
                errPipe.fileHandleForReading.readabilityHandler = nil

                let reason: String
                if proc.terminationReason == .exit {
                    reason = "exit:\(proc.terminationStatus)"
                } else {
                    reason = "signal:\(proc.terminationStatus)"
                }
                self.serviceMenuItem.title = "Borg service: stopped (\(reason))"
                self.refreshServiceMenuState()
                print("borg_menu_spike:borg_stopped=\(reason)")
                fflush(stdout)

                if self.restartRequested {
                    self.restartRequested = false
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
                        self.startBorgService()
                    }
                }
            }
        }

        do {
            try process.run()
            borgProcess = process
            serviceMenuItem.title = "Borg service: running (pid \(process.processIdentifier))"
            refreshServiceMenuState()
            print("borg_menu_spike:borg_started path=\(path) args=\(borgArguments.joined(separator: " "))")
            fflush(stdout)
        } catch {
            serviceMenuItem.title = "Borg service: error (\(error.localizedDescription))"
            refreshServiceMenuState()
            print("borg_menu_spike:borg_error=\(error.localizedDescription)")
            fflush(stdout)
        }
    }

    private func stopBorgService(forceKillAfterSeconds: Double) {
        guard let process = borgProcess, process.isRunning else {
            refreshServiceMenuState()
            return
        }

        serviceMenuItem.title = "Borg service: stopping..."
        refreshServiceMenuState()
        process.terminate()

        if forceKillAfterSeconds > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + forceKillAfterSeconds) { [weak self] in
                guard let self else { return }
                guard let process = self.borgProcess, process.isRunning else {
                    return
                }
                process.interrupt()
                _ = Darwin.kill(process.processIdentifier, SIGKILL)
            }
        }
    }

    private func resolveBorgBinaryPath() -> String? {
        if let configured = borgPath,
           FileManager.default.isExecutableFile(atPath: configured)
        {
            return configured
        }

        let cwd = FileManager.default.currentDirectoryPath
        let candidates = [
            "\(cwd)/target/debug/borg-cli",
            "\(cwd)/target/release/borg-cli",
            "\(cwd)/target/debug/borg",
            "\(cwd)/target/release/borg",
            "/opt/homebrew/bin/borg",
            "/usr/local/bin/borg",
            "/usr/bin/borg",
        ]
        for candidate in candidates where FileManager.default.isExecutableFile(atPath: candidate) {
            return candidate
        }

        if let gitRoot = resolveGitTopLevelPath() {
            let rootCandidates = [
                "\(gitRoot)/target/debug/borg-cli",
                "\(gitRoot)/target/release/borg-cli",
                "\(gitRoot)/target/debug/borg",
                "\(gitRoot)/target/release/borg",
            ]
            for candidate in rootCandidates where FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }

        return resolveBinaryWithWhich("borg-cli") ?? resolveBinaryWithWhich("borg")
    }

    private func transitionToWaiting(_ status: String) {
        state = .waiting
        setStatusBubble(color: .systemGray, text: "Borg")
        statusMenuItem.title = "Status: \(status)"
        print("borg_menu_spike:state=waiting")
        fflush(stdout)
    }

    private func transitionToRecording() {
        state = .recording
        setStatusBubble(color: .systemGreen, text: "Borg")
        statusMenuItem.title = "Status: Recording..."
        print("borg_menu_spike:state=recording")
        fflush(stdout)
    }

    private func transitionToError(_ message: String) {
        state = .error
        setStatusBubble(color: .systemRed, text: "Borg")
        statusMenuItem.title = "Status: Error"
        transcriptMenuItem.title = "Last transcript: \(message)"
        print("borg_menu_spike:error=\(message)")
        fflush(stdout)
    }

    private func isCancellationError(_ error: Error) -> Bool {
        let lower = error.localizedDescription.lowercased()
        return lower.contains("canceled") || lower.contains("cancelled")
    }

    private func isNoSpeechError(_ error: Error) -> Bool {
        let lower = error.localizedDescription.lowercased()
        if lower.contains("no speech detected") || lower.contains("speech not detected") {
            return true
        }
        let nsError = error as NSError
        if nsError.domain == "kAFAssistantErrorDomain" && nsError.code == 1110 {
            return true
        }
        return false
    }

    private func wakePhraseMatchRanges(in transcript: String) -> [Range<String.Index>] {
        guard let regex = wakePhraseRegex else {
            return []
        }
        let nsRange = NSRange(transcript.startIndex..<transcript.endIndex, in: transcript)
        let matches = regex.matches(in: transcript, options: [], range: nsRange)
        return matches.compactMap { match in
            Range(match.range, in: transcript)
        }
    }

    private func wakePhraseParts(
        in transcript: String
    ) -> (prefix: String, between: [String], suffix: String)? {
        let ranges = wakePhraseMatchRanges(in: transcript)
        guard !ranges.isEmpty else {
            return nil
        }
        let first = ranges[0]
        let prefix = transcript[..<first.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)

        var between = [String]()
        if ranges.count > 1 {
            for idx in 0..<(ranges.count - 1) {
                let start = ranges[idx].upperBound
                let end = ranges[idx + 1].lowerBound
                let chunk = transcript[start..<end].trimmingCharacters(in: .whitespacesAndNewlines)
                between.append(chunk)
            }
        }

        let suffix = transcript[ranges[ranges.count - 1].upperBound...]
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return (prefix, between, suffix)
    }

    private func logWakeParts(prefix: String, between: [String], suffix: String) {
        let prefixValue = prefix.isEmpty ? "<empty>" : prefix
        let suffixValue = suffix.isEmpty ? "<empty>" : suffix
        print("borg_menu_spike:wake_prefix=\(prefixValue)")
        if between.isEmpty {
            print("borg_menu_spike:wake_between=<none>")
        } else {
            for (idx, chunk) in between.enumerated() {
                let value = chunk.isEmpty ? "<empty>" : chunk
                print("borg_menu_spike:wake_between[\(idx)]=\(value)")
            }
        }
        print("borg_menu_spike:wake_buffer=\(suffixValue)")
        fflush(stdout)
    }

    private func setStatusBubble(color: NSColor, text: String) {
        let composed = NSMutableAttributedString(string: "● \(text)")
        composed.addAttribute(
            .foregroundColor,
            value: color,
            range: NSRange(location: 0, length: 1)
        )
        statusItem?.button?.attributedTitle = composed
    }

    @objc private func openDashboard() {
        _ = NSWorkspace.shared.open(URL(string: "http://127.0.0.1:18080/dashboard")!)
    }

    @objc private func openLoginItemsSettings() {
        guard let url = URL(
            string: "x-apple.systempreferences:com.apple.LoginItems-Settings.extension"
        ) else {
            return
        }
        _ = NSWorkspace.shared.open(url)
    }

    @objc private func quit() {
        NSApplication.shared.terminate(nil)
    }
}

@main
struct BorgMenuSpikeMain {
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)

        let delegate = SpikeAppDelegate()
        app.delegate = delegate

        let env = ProcessInfo.processInfo.environment
        let autoSeconds = Double(env["BORG_MENU_SPIKE_AUTOTERMINATE_SECONDS"] ?? "0") ?? 0
        if autoSeconds > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + autoSeconds) {
                print("borg_menu_spike:auto_terminate")
                fflush(stdout)
                app.terminate(nil)
            }
        }

        app.run()
    }
}

private func waitSpeechAuthorization() -> Bool {
    let status = SFSpeechRecognizer.authorizationStatus()
    if status == .authorized {
        return true
    }
    if status == .denied || status == .restricted {
        return false
    }

    let sem = DispatchSemaphore(value: 0)
    let granted = LockedBool(false)
    SFSpeechRecognizer.requestAuthorization { newStatus in
        granted.set(newStatus == .authorized)
        sem.signal()
    }
    sem.wait()
    return granted.get()
}

private func parseEnvBool(_ value: String?) -> Bool {
    guard let value else { return false }
    switch value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
    case "1", "true", "yes", "on":
        return true
    default:
        return false
    }
}

private func normalizeWakeString(_ text: String) -> String {
    let lowered = text.lowercased()
    let mapped = lowered.map { ch -> Character in
        if ch.isLetter || ch.isNumber {
            return ch
        }
        return " "
    }
    return String(mapped)
        .split(whereSeparator: \.isWhitespace)
        .joined(separator: " ")
}

private func buildWakePhraseRegex(_ normalizedPhrase: String) -> NSRegularExpression? {
    let tokens = normalizedPhrase
        .split(whereSeparator: \.isWhitespace)
        .map(String.init)
    guard !tokens.isEmpty else {
        return nil
    }
    let pattern = "\\b" + tokens
        .map { NSRegularExpression.escapedPattern(for: $0) }
        .joined(separator: "\\W+") + "\\b"
    return try? NSRegularExpression(
        pattern: pattern,
        options: [.caseInsensitive]
    )
}

private func resolveBinaryWithWhich(_ binary: String) -> String? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
    process.arguments = [binary]

    let out = Pipe()
    process.standardOutput = out
    process.standardError = Pipe()

    do {
        try process.run()
    } catch {
        return nil
    }

    process.waitUntilExit()
    if process.terminationStatus != 0 {
        return nil
    }

    let data = out.fileHandleForReading.readDataToEndOfFile()
    guard let raw = String(data: data, encoding: .utf8)?
        .trimmingCharacters(in: .whitespacesAndNewlines),
        !raw.isEmpty
    else {
        return nil
    }
    return raw
}

private func resolveGitTopLevelPath() -> String? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    process.arguments = ["rev-parse", "--show-toplevel"]

    let out = Pipe()
    process.standardOutput = out
    process.standardError = Pipe()

    do {
        try process.run()
    } catch {
        return nil
    }

    process.waitUntilExit()
    if process.terminationStatus != 0 {
        return nil
    }

    let data = out.fileHandleForReading.readDataToEndOfFile()
    guard let root = String(data: data, encoding: .utf8)?
        .trimmingCharacters(in: .whitespacesAndNewlines),
        !root.isEmpty
    else {
        return nil
    }
    return root
}

private func waitMicrophoneAuthorization() -> Bool {
    let status = AVCaptureDevice.authorizationStatus(for: .audio)
    if status == .authorized {
        return true
    }
    if status == .denied || status == .restricted {
        return false
    }

    let sem = DispatchSemaphore(value: 0)
    let granted = LockedBool(false)
    AVCaptureDevice.requestAccess(for: .audio) { ok in
        granted.set(ok)
        sem.signal()
    }
    sem.wait()
    return granted.get()
}

private final class LockedBool: @unchecked Sendable {
    private var value: Bool
    private let lock = NSLock()

    init(_ value: Bool) {
        self.value = value
    }

    func set(_ next: Bool) {
        lock.lock()
        value = next
        lock.unlock()
    }

    func get() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}
