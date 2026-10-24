// `gn-drive serve --stdio` — hosts the engine for the GPUI window (rpc.md v1).
import Foundation
import ArgumentParser
import GNDriveCore

struct ServeCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "serve",
        abstract: "Serve the newline-delimited JSON RPC protocol on stdio (protocol v1)")

    @Flag(help: "Required in v1: serve the protocol on stdin/stdout")
    var stdio = false

    @Option(help: "Override the config directory")
    var configDir: String = ""

    @Option(help: "Override the rclone binary")
    var rclone: String = ""

    @Option(help: "Minimum log level: debug|info|warn|error")
    var logLevel: String = "info"

    @Option(help: "In-memory log ring size for logs.read")
    var logBuffer: Int = 2000

    @Option(help: "Unlock at startup (automation; prefer auth.unlock)")
    var password: String = ""

    @Option(name: .customLong("protocol"), help: "Protocol version (only 1 is supported)")
    var protocolVersion: Int = 1

    func run() throws {
        signal(SIGPIPE, SIG_IGN)

        guard stdio else {
            writeStderr("gn-drive serve: --stdio is required in protocol v1\n")
            throw ExitCode(2)
        }
        guard protocolVersion == 1 else {
            RPCService.emitFatal(code: "unsupported_protocol",
                                 message: "unsupported protocol version \(protocolVersion) (want 1)")
            throw ExitCode(2)
        }

        let level = LogLevel(label: logLevel) ?? .info

        if RPCService.isAppRunning() {
            RPCService.emitFatal(
                code: "instance_busy",
                message: "the GN Drive app is running. Close it before starting another window.",
                holder: "app")
            throw ExitCode(3)
        }

        let cfgDir = configDir.isEmpty ? Paths.detect().configDir : configDir
        let locker: InstanceLocker
        do {
            locker = try InstanceLocker.acquire(configDir: cfgDir)
        } catch InstanceError.anotherInstance(let pid) {
            let holder = pid.map(RPCService.classifyHolder(pid:)) ?? "cli"
            var message = "another gn-drive instance is running"
            if let pid { message += " (pid=\(pid), holder=\(holder))" }
            message += ". Stop it with 'gn-drive service stop' or close the other window."
            RPCService.emitFatal(code: "instance_busy", message: message, pid: pid, holder: holder)
            throw ExitCode(3)
        }
        defer { locker.release() }

        var opts = AppContainer.Options()
        opts.portalMode = true
        opts.keyStore = KeychainStore(service: "gn-drive")
        opts.version = BuildInfo.version
        opts.logMode = .foreground
        opts.configDir = configDir
        opts.rcloneBinary = rclone
        let envPassword = ProcessInfo.processInfo.environment["GN_DRIVE_PASSWORD"] ?? ""
        opts.unlockPassword = password.isEmpty ? envPassword : password

        let container: AppContainer
        do {
            container = try AppContainer(opts: opts)
        } catch AppError.rcloneMissing(let underlying) {
            RPCService.emitFatal(code: "rclone_missing", message: "rclone unavailable: \(underlying)")
            throw ExitCode(2)
        } catch AppError.locked {
            RPCService.emitFatal(code: "locked", message: "auth: app is locked")
            throw ExitCode(2)
        } catch {
            RPCService.emitFatal(code: "init_failed", message: "initialization failed: \(error)")
            throw ExitCode(2)
        }

        var serviceOptions = RPCService.Options()
        serviceOptions.logLevel = level
        serviceOptions.logBufferCapacity = max(1, logBuffer)
        let service = RPCService(container: container,
                                 lockPath: (cfgDir as NSString).appendingPathComponent("gn-drive.lock"),
                                 locker: locker,
                                 rcloneBinary: rclone,
                                 options: serviceOptions)

        let signalQueue = DispatchQueue(label: "gn-drive.rpc.signal")
        var signalSources: [DispatchSourceSignal] = []
        for sig in [SIGINT, SIGTERM] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: signalQueue)
            source.setEventHandler { service.beginShutdown(reason: "signal") }
            source.resume()
            signalSources.append(source)
        }

        container.syncEngine.start()
        withExtendedLifetime(signalSources) {
            _ = service.run()
        }
    }

    private func writeStderr(_ text: String) {
        FileHandle.standardError.write(Data(text.utf8))
    }
}
