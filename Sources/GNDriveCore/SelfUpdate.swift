// Self-update via GitHub Releases — port of internal/selfupdate.
// Pipeline: fetch latest release → pick gn-drive-darwin-<arch>.tar.gz +
// .sha256 sidecar → download → verify SHA256 → extract → atomic swap.
import Foundation
import CryptoKit

public enum SelfUpdateError: Error, LocalizedError {
    case noRelease
    case noMatchingAsset(String)
    case missingChecksum(String)
    case checksumMismatch
    case alreadyUpToDate
    case http(Int, String)
    case extractFailed(String)

    public var errorDescription: String? {
        switch self {
        case .noRelease: return "selfupdate: no release found"
        case .noMatchingAsset(let n): return "selfupdate: no asset matches current platform: \(n)"
        case .missingChecksum(let n): return "selfupdate: missing checksum asset \(n)"
        case .checksumMismatch: return "selfupdate: SHA256 mismatch"
        case .alreadyUpToDate: return "selfupdate: already on latest version"
        case .http(let code, let body): return "selfupdate: HTTP \(code): \(body)"
        case .extractFailed(let m): return "selfupdate: extract: \(m)"
        }
    }
}

public struct GitHubRelease: Decodable {
    public let tag_name: String
    public let assets: [Asset]

    public struct Asset: Decodable {
        public let name: String
        public let browser_download_url: String
    }
}

public struct UpdateResult: Sendable {
    public var updated: Bool = false
    public var oldVersion = ""
    public var newVersion = ""
    public var binaryPath = ""
    public var restartHint = ""
}

public struct UpdateOptions {
    public var repoOwner = "gnasdev"
    public var repoName = "gn-drive"
    public var currentVersion = ""
    public var force = false
    public var token: String? = ProcessInfo.processInfo.environment["GITHUB_TOKEN"]
    public var log: (String) -> Void = { print($0) }
    public init() {}
}

public enum SelfUpdate {
    static let apiBase = "https://api.github.com"
    static let httpTimeout: TimeInterval = 60

    /// Check for a newer release without downloading.
    public static func check(opts: UpdateOptions) throws -> (current: String, latest: String) {
        let rel = try fetchRelease(opts: opts)
        return (opts.currentVersion, rel.tag_name.trimmingPrefix("v"))
    }

    /// Full pipeline. Throws .alreadyUpToDate when versions match.
    public static func update(opts: UpdateOptions) throws -> UpdateResult {
        let rel = try fetchRelease(opts: opts)
        let newVersion = rel.tag_name.trimmingPrefix("v")

        if !opts.force, !opts.currentVersion.isEmpty, newVersion == opts.currentVersion {
            throw SelfUpdateError.alreadyUpToDate
        }

        let (asset, sumAsset) = try pickAssets(rel)
        let stage = try FileManager.default.createTempDir(prefix: "gn-drive-update-")
        defer { try? FileManager.default.removeItem(atPath: stage) }

        let archivePath = (stage as NSString).appendingPathComponent(asset.name)
        opts.log("↓ downloading \(asset.name)")
        try download(url: asset.browser_download_url, to: archivePath, token: opts.token)

        let expectedSum = try fetchChecksum(opts: opts, asset: sumAsset, archivePath: archivePath)
        try verifyFile(archivePath, expectedHex: expectedSum)

        let binaryPath = try extractBinary(archivePath: archivePath, destDir: stage)

        var currentPath = try currentBinaryPath()
        currentPath = (currentPath as NSString).resolvingSymlinksInPath

        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: binaryPath)
        try atomicSwap(newBin: binaryPath, currentBin: currentPath)

        return UpdateResult(
            updated: true, oldVersion: opts.currentVersion, newVersion: newVersion,
            binaryPath: currentPath,
            restartHint: "Restart gn-drive to use the new binary. Foreground: Ctrl+C and re-run. Service: 'gn-drive service restart'.")
    }

    // MARK: - Internals

    private static func fetchRelease(opts: UpdateOptions) throws -> GitHubRelease {
        let url = "\(apiBase)/repos/\(opts.repoOwner)/\(opts.repoName)/releases/latest"
        var req = URLRequest(url: URL(string: url)!)
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        if let token = opts.token, !token.isEmpty {
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        req.timeoutInterval = httpTimeout

        let (data, resp) = try URLSession.shared.synchronousDataTask(with: req)
        let code = (resp as? HTTPURLResponse)?.statusCode ?? -1
        if code == 404 { throw SelfUpdateError.noRelease }
        guard code == 200 else {
            let body = String(data: data.prefix(1024), encoding: .utf8) ?? ""
            throw SelfUpdateError.http(code, body)
        }
        guard let rel = try? JSONDecoder().decode(GitHubRelease.self, from: data),
              !rel.tag_name.isEmpty else {
            throw SelfUpdateError.noRelease
        }
        return rel
    }

    private static func pickAssets(_ r: GitHubRelease) throws -> (GitHubRelease.Asset, GitHubRelease.Asset) {
        #if arch(arm64)
        let arch = "arm64"
        #else
        let arch = "x86_64"  // amd64 naming? Go uses runtime.GOARCH=amd64
        #endif
        // Go releases used "darwin_arm64" / "darwin_amd64" naming.
        let goArch = arch == "x86_64" ? "amd64" : arch
        let base = "gn-drive-darwin-\(goArch).tar.gz"
        guard let bin = r.assets.first(where: { $0.name == base }) else {
            throw SelfUpdateError.noMatchingAsset(base)
        }
        let sumName = base + ".sha256"
        guard let sum = r.assets.first(where: { $0.name == sumName }) else {
            throw SelfUpdateError.missingChecksum(sumName)
        }
        return (bin, sum)
    }

    private static func download(url: String, to dest: String, token: String?) throws {
        var req = URLRequest(url: URL(string: url)!)
        if let token, !token.isEmpty {
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        req.timeoutInterval = 300
        let (data, resp) = try URLSession.shared.synchronousDataTask(with: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            throw SelfUpdateError.http((resp as? HTTPURLResponse)?.statusCode ?? -1, "")
        }
        try data.write(to: URL(fileURLWithPath: dest))
    }

    private static func fetchChecksum(opts: UpdateOptions, asset: GitHubRelease.Asset,
                                      archivePath: String) throws -> String {
        var req = URLRequest(url: URL(string: asset.browser_download_url)!)
        if let token = opts.token, !token.isEmpty {
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (data, resp) = try URLSession.shared.synchronousDataTask(with: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            throw SelfUpdateError.http((resp as? HTTPURLResponse)?.statusCode ?? -1, "")
        }
        // Format: "<sha256hex>  <filename>"
        let text = String(data: data, encoding: .utf8) ?? ""
        return text.split(separator: " ").first.map(String.init) ?? ""
    }

    private static func verifyFile(_ path: String, expectedHex: String) throws {
        let data = try Data(contentsOf: URL(fileURLWithPath: path))
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard digest == expectedHex.trimmingCharacters(in: .whitespaces) else {
            throw SelfUpdateError.checksumMismatch
        }
    }

    /// Extract gn-drive binary from tar.gz (or zip via /usr/bin/unzip fallback).
    private static func extractBinary(archivePath: String, destDir: String) throws -> String {
        if archivePath.hasSuffix(".tar.gz") || archivePath.hasSuffix(".tgz") {
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
            proc.arguments = ["-xzf", archivePath, "-C", destDir]
            try proc.run()
            proc.waitUntilExit()
            guard proc.terminationStatus == 0 else {
                throw SelfUpdateError.extractFailed("tar exit \(proc.terminationStatus)")
            }
        } else if archivePath.hasSuffix(".zip") {
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
            proc.arguments = ["-o", archivePath, "-d", destDir]
            try proc.run()
            proc.waitUntilExit()
            guard proc.terminationStatus == 0 else {
                throw SelfUpdateError.extractFailed("unzip exit \(proc.terminationStatus)")
            }
        } else {
            throw SelfUpdateError.extractFailed("unsupported archive: \(archivePath)")
        }
        // Find the extracted binary.
        let contents = (try? FileManager.default.contentsOfDirectory(atPath: destDir)) ?? []
        for name in contents where name == "gn-drive" {
            return (destDir as NSString).appendingPathComponent(name)
        }
        // Fall back: any executable file.
        for name in contents {
            let p = (destDir as NSString).appendingPathComponent(name)
            var isDir: ObjCBool = false
            if FileManager.default.fileExists(atPath: p, isDirectory: &isDir), !isDir.boolValue,
               FileManager.default.isExecutableFile(atPath: p), !name.hasSuffix(".tar.gz") {
                return p
            }
        }
        throw SelfUpdateError.extractFailed("no binary found in archive")
    }

    private static func currentBinaryPath() throws -> String {
        Bundle.main.executablePath ?? CommandLine.arguments[0]
    }

    /// Atomic binary replacement (POSIX rename; keeps <bin>.bak).
    private static func atomicSwap(newBin: String, currentBin: String) throws {
        let bak = currentBin + ".bak"
        try? FileManager.default.removeItem(atPath: bak)
        // Move current to .bak, then move new into place.
        try FileManager.default.moveItem(atPath: currentBin, toPath: bak)
        do {
            try FileManager.default.moveItem(atPath: newBin, toPath: currentBin)
        } catch {
            try? FileManager.default.moveItem(atPath: bak, toPath: currentBin)
            throw error
        }
    }
}

extension FileManager {
    func createTempDir(prefix: String) throws -> String {
        let dir = NSTemporaryDirectory() + prefix + UUID().uuidString
        try createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }
}

// URLSession synchronous helper (selfupdate is a CLI path; blocking is fine).
extension URLSession {
    func synchronousDataTask(with request: URLRequest) throws -> (Data, URLResponse) {
        var result: Result<(Data, URLResponse), Error>!
        let sem = DispatchSemaphore(value: 0)
        let task = dataTask(with: request) { data, resp, err in
            if let err { result = .failure(err) }
            else { result = .success((data ?? Data(), resp!)) }
            sem.signal()
        }
        task.resume()
        sem.wait()
        return try result.get()
    }
}

extension String {
    func trimmingPrefix(_ p: String) -> String {
        hasPrefix(p) ? String(dropFirst(p.count)) : self
    }
}
