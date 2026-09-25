// Master password auth: Argon2id + AES-256-GCM — port of internal/auth/auth.go.
// Keeps the same auth.json wire format so existing data loads transparently.
import Foundation

public enum AuthError: Error, Equatable {
    case notSetup
    case alreadyUnlocked
    case notUnlocked
    case invalidPassword
    case locked(retryAfterSecs: Int)
    case alreadySetup
    case passwordTooShort
}

public struct AppSettings: Codable, Sendable {
    public var notificationsEnabled: Bool = false
    public var debugMode: Bool = false
    public var minimizeToTray: Bool = false
    public var startAtLogin: Bool = false

    public init() {}

    enum CodingKeys: String, CodingKey {
        case notificationsEnabled = "notifications_enabled"
        case debugMode = "debug_mode"
        case minimizeToTray = "minimize_to_tray"
        case startAtLogin = "start_at_login"
    }
}

public struct AuthData: Codable, Sendable {
    public var enabled: Bool = false
    public var passwordHash: String = ""
    public var failedAttempts: Int = 0
    public var lockoutUntil: String = ""
    public var appSettings: AppSettings = AppSettings()

    enum CodingKeys: String, CodingKey {
        case enabled
        case passwordHash = "password_hash"
        case failedAttempts = "failed_attempts"
        case lockoutUntil = "lockout_until"
        case appSettings = "app_settings"
    }
}

public struct LockoutStatus: Sendable {
    public var failedAttempts: Int = 0
    public var lockedUntil: String = ""
    public var isLocked: Bool = false
    public var retryAfterSecs: Int = 0
}

public struct AuthStatus: Sendable {
    public var setup: Bool = false
    public var unlocked: Bool = false
    public var lockedAt: Date? = nil
    public var enabled: Bool = false
    public var lockout: LockoutStatus = LockoutStatus()
}

/// Manages password-based unlock, config encryption, and rate limiting.
public final class AuthService: @unchecked Sendable {
    private static let maxAttemptsBeforeDelay = 3
    private static let maxAttemptsBeforeLock = 10
    private static let lockoutDuration: TimeInterval = 5 * 60

    private let mutex = NSRecursiveLock()
    private(set) var unlocked = false
    private var encKey: Data?
    private var authData: AuthData
    private let authFile: String
    private let configDir: String
    private let logger: Logger
    private let keyStore: SecureStore?
    private let keyAccount: String
    private var lockedAt: Date?
    /// Injectable sleep (tests).
    var sleep: (TimeInterval) -> Void = { Thread.sleep(forTimeInterval: $0) }

    public init(configDir: String, logger: Logger, keyStore: SecureStore? = nil) throws {
        guard !configDir.isEmpty else { throw AuthError.notSetup }
        self.authFile = (configDir as NSString).appendingPathComponent("auth.json")
        self.configDir = configDir
        self.logger = logger
        self.keyStore = keyStore
        self.keyAccount = AuthService.rememberedKeyAccount(configDir: configDir)
        if let data = try? Data(contentsOf: URL(fileURLWithPath: authFile)),
           let d = try? JSONDecoder().decode(AuthData.self, from: data) {
            self.authData = d
        } else {
            self.authData = AuthData(enabled: false)
        }

        recoverFromCrash()

        if !authData.enabled {
            unlocked = true
            logger.info("auth: not configured, app is open")
        } else {
            unlocked = false
            resumeRememberedKey()
            logger.info(unlocked ? "auth: resumed from OS keychain" : "auth: configured, app is locked")
        }
    }

    public var isSetup: Bool { mutex.lock(); defer { mutex.unlock() }; return authData.enabled }
    public var isUnlocked: Bool { mutex.lock(); defer { mutex.unlock() }; return unlocked }

    /// Any config files still encrypted on disk (.enc)?
    public func hasEncryptedConfig() -> Bool {
        for name in ["rclone.conf.enc", "gn-drive.db.enc"] {
            if FileManager.default.fileExists(atPath: (configDir as NSString).appendingPathComponent(name)) {
                return true
            }
        }
        return false
    }

    /// Marks the app unlocked without a decryption key. Only valid when no
    /// .enc files exist (config already plaintext — dev restart / crash path).
    public func openPlaintextSession() throws {
        mutex.lock(); defer { mutex.unlock() }
        if !authData.enabled || unlocked { unlocked = true; return }
        for name in ["rclone.conf.enc", "gn-drive.db.enc"] {
            if FileManager.default.fileExists(atPath: (configDir as NSString).appendingPathComponent(name)) {
                throw AuthError.notUnlocked
            }
        }
        unlocked = true
        logger.info("auth: opened plaintext session (no re-encrypt key)")
    }

    public func status() -> AuthStatus {
        mutex.lock(); defer { mutex.unlock() }
        return AuthStatus(setup: authData.enabled, unlocked: unlocked,
                          lockedAt: lockedAt, enabled: authData.enabled,
                          lockout: lockoutStatusLocked())
    }

    public func lockoutStatus() -> LockoutStatus {
        mutex.lock(); defer { mutex.unlock() }
        return lockoutStatusLocked()
    }

    private func lockoutStatusLocked() -> LockoutStatus {
        var status = LockoutStatus(failedAttempts: authData.failedAttempts,
                                   lockedUntil: authData.lockoutUntil)
        if !authData.lockoutUntil.isEmpty,
           let t = Self.rfc3339(authData.lockoutUntil), Date() < t {
            status.isLocked = true
            status.retryAfterSecs = Int(ceil(t.timeIntervalSinceNow))
        }
        if !status.isLocked && authData.failedAttempts >= Self.maxAttemptsBeforeDelay {
            status.retryAfterSecs = Int(pow(2.0, Double(authData.failedAttempts - Self.maxAttemptsBeforeDelay)))
        }
        return status
    }

    public func setupPassword(_ password: String) throws {
        mutex.lock(); defer { mutex.unlock() }
        if authData.enabled { throw AuthError.alreadySetup }
        if password.count < 4 { throw AuthError.passwordTooShort }

        let hash = try Argon2.createHash(password: password)
        let salt = try Argon2.extractSalt(encoded: hash)
        let key = Argon2.deriveKey(password: password, salt: salt)

        authData = AuthData(enabled: true, passwordHash: hash,
                            failedAttempts: 0, lockoutUntil: "",
                            appSettings: authData.appSettings)
        try saveAuthData()
        encKey = key
        unlocked = true
        rememberKey(key)
        logger.info("auth: password set up")
    }

    public func unlock(_ password: String) throws {
        mutex.lock()
        if !authData.enabled { mutex.unlock(); throw AuthError.notSetup }
        if unlocked { mutex.unlock(); return }

        if !authData.lockoutUntil.isEmpty,
           let t = Self.rfc3339(authData.lockoutUntil), Date() < t {
            let remaining = Int(ceil(t.timeIntervalSinceNow))
            mutex.unlock()
            throw AuthError.locked(retryAfterSecs: remaining)
        }
        authData.lockoutUntil = ""

        if authData.failedAttempts >= Self.maxAttemptsBeforeDelay
            && authData.failedAttempts < Self.maxAttemptsBeforeLock {
            let delay = pow(2.0, Double(authData.failedAttempts - Self.maxAttemptsBeforeDelay))
            mutex.unlock()
            sleep(delay)
            mutex.lock()
            if unlocked { mutex.unlock(); return }
        }

        guard Argon2.verify(password: password, encoded: authData.passwordHash) else {
            authData.failedAttempts += 1
            if authData.failedAttempts >= Self.maxAttemptsBeforeLock {
                authData.lockoutUntil = Self.rfc3339(Date().addingTimeInterval(Self.lockoutDuration))
                authData.failedAttempts = 0
                logger.warn("auth: too many failed attempts, locked")
            }
            try? saveAuthData()
            mutex.unlock()
            throw AuthError.invalidPassword
        }

        let salt = try Argon2.extractSalt(encoded: authData.passwordHash)
        let key = Argon2.deriveKey(password: password, salt: salt)
        do {
            try decryptConfigFiles(key: key)
        } catch {
            mutex.unlock()
            throw error
        }

        authData.failedAttempts = 0
        authData.lockoutUntil = ""
        try? saveAuthData()
        encKey = key
        unlocked = true
        lockedAt = nil
        rememberKey(key)
        mutex.unlock()
        logger.info("auth: unlocked")
    }

    /// Re-encrypt files, remove remembered key, zero the key.
    public func lock() throws {
        self.mutex.lock()
        if unlocked { lockInternal() }
        self.mutex.unlock()
        try forgetRememberedKey()
    }

    /// Re-encrypt files before shutdown but keep the remembered key so a later
    /// process owned by the same OS user can resume.
    public func suspend() {
        mutex.lock(); defer { mutex.unlock() }
        if unlocked { lockInternal() }
    }

    private func lockInternal() {
        guard unlocked, let key = encKey else { return }
        do { try encryptConfigFiles(key: key) }
        catch { logger.error("auth: encrypt on lock failed", err: error) }
        var k: Data? = key
        zeroBytes(&k)
        encKey = nil
        unlocked = false
        lockedAt = Date()
    }

    public func changePassword(old: String, new: String) throws {
        mutex.lock(); defer { mutex.unlock() }
        if !authData.enabled { throw AuthError.notSetup }
        if !unlocked { throw AuthError.notUnlocked }
        guard Argon2.verify(password: old, encoded: authData.passwordHash) else {
            throw AuthError.invalidPassword
        }
        if new.count < 4 { throw AuthError.passwordTooShort }
        try forgetRememberedKey()

        let newHash = try Argon2.createHash(password: new)
        let newSalt = try Argon2.extractSalt(encoded: newHash)
        let newKey = Argon2.deriveKey(password: new, salt: newSalt)

        do {
            try encryptConfigFiles(key: newKey)
        } catch {
            // Recover: decrypt with new key.
            if (try? decryptConfigFiles(key: newKey)) == nil {
                var k: Data? = encKey; zeroBytes(&k); encKey = nil; unlocked = false
            }
            throw error
        }

        let oldHash = authData.passwordHash
        authData.passwordHash = newHash
        do {
            try saveAuthData()
        } catch {
            authData.passwordHash = oldHash
            try? decryptConfigFiles(key: newKey)
            throw error
        }

        try decryptConfigFiles(key: newKey)
        var oldK: Data? = encKey; zeroBytes(&oldK)
        encKey = newKey
        logger.info("auth: password changed")
    }

    public func removePassword(_ password: String) throws {
        mutex.lock(); defer { mutex.unlock() }
        if !authData.enabled { throw AuthError.notSetup }
        if !unlocked { throw AuthError.notUnlocked }
        guard Argon2.verify(password: password, encoded: authData.passwordHash) else {
            throw AuthError.invalidPassword
        }
        cleanupEncryptedFiles()
        try? FileManager.default.removeItem(atPath: authFile)
        var k: Data? = encKey; zeroBytes(&k); encKey = nil
        authData = AuthData(enabled: false)
        unlocked = true
        logger.info("auth: password removed")
    }

    /// Read user-facing settings persisted in auth.json.
    public func appSettings() -> AppSettings {
        mutex.lock(); defer { mutex.unlock() }
        return authData.appSettings
    }

    public func setAppSettings(_ s: AppSettings) throws {
        mutex.lock(); defer { mutex.unlock() }
        authData.appSettings = s
        try saveAuthData()
    }

    // --- Internal ----------------------------------------------------------

    private func saveAuthData() throws {
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try enc.encode(authData)
        try FileManager.default.createDirectory(
            atPath: (authFile as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true)
        try data.write(to: URL(fileURLWithPath: authFile), options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: authFile)
    }

    /// If both plaintext and .enc exist: auth enabled → keep .enc; disabled →
    /// keep plaintext.
    private func recoverFromCrash() {
        for name in ["rclone.conf", "gn-drive.db"] {
            let base = (configDir as NSString).appendingPathComponent(name)
            let enc = base + ".enc"
            let fm = FileManager.default
            guard fm.fileExists(atPath: base), fm.fileExists(atPath: enc) else { continue }
            if authData.enabled {
                try? fm.removeItem(atPath: base)
                try? fm.removeItem(atPath: base + "-wal")
                try? fm.removeItem(atPath: base + "-shm")
                logger.warn("auth: crash recovery - removed plaintext", ("file", name))
            } else {
                try? fm.removeItem(atPath: enc)
                logger.warn("auth: crash recovery - removed .enc", ("file", name))
            }
        }
    }

    private func encryptConfigFiles(key: Data) throws {
        for name in ["rclone.conf", "gn-drive.db"] {
            let src = (configDir as NSString).appendingPathComponent(name)
            guard FileManager.default.fileExists(atPath: src) else { continue }
            try FileCrypto.encryptFile(src: src, dst: src + ".enc", key: key)
            try? FileManager.default.removeItem(atPath: src)
            try? FileManager.default.removeItem(atPath: src + "-wal")
            try? FileManager.default.removeItem(atPath: src + "-shm")
        }
    }

    private func decryptConfigFiles(key: Data) throws {
        for name in ["rclone.conf", "gn-drive.db"] {
            let base = (configDir as NSString).appendingPathComponent(name)
            let enc = base + ".enc"
            guard FileManager.default.fileExists(atPath: enc) else { continue }
            try FileCrypto.decryptFile(src: enc, dst: base, key: key)
            try? FileManager.default.removeItem(atPath: enc)
        }
    }

    private func cleanupEncryptedFiles() {
        for name in ["rclone.conf.enc", "gn-drive.db.enc"] {
            try? FileManager.default.removeItem(atPath: (configDir as NSString).appendingPathComponent(name))
        }
    }

    private static func rememberedKeyAccount(configDir: String) -> String {
        let canonical = (configDir as NSString).standardizingPath
        var digest = [UInt8](repeating: 0, count: 32)
        canonical.data(using: .utf8)!.withUnsafeBytes { ptr in
            digest = Array(CryptoKitSHA256(ptr))
        }
        return "remember-unlock/v1/" + digest.map { String(format: "%02x", $0) }.joined()
    }

    private func resumeRememberedKey() {
        guard let keyStore, hasEncryptedConfig() else { return }
        guard let key = try? keyStore.get(account: keyAccount) else { return }
        guard key.count == Argon2.keyLength else {
            logger.warn("auth: remembered key has invalid length")
            try? keyStore.delete(account: keyAccount)
            return
        }
        do {
            try decryptConfigFiles(key: key)
        } catch {
            var k: Data? = key; zeroBytes(&k)
            logger.warn("auth: remembered key could not decrypt config", err: error)
            try? keyStore.delete(account: keyAccount)
            return
        }
        encKey = key
        unlocked = true
        lockedAt = nil
    }

    private func rememberKey(_ key: Data) {
        guard let keyStore else { return }
        do { try keyStore.set(account: keyAccount, value: key) }
        catch { logger.warn("auth: store remembered key", err: error) }
    }

    private func forgetRememberedKey() throws {
        try keyStore?.delete(account: keyAccount)
    }

    // --- Time helpers ------------------------------------------------------

    static func rfc3339(_ d: Date) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f.string(from: d)
    }

    static func rfc3339(_ s: String) -> Date? {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f.date(from: s)
    }
}

import CryptoKit
private func CryptoKitSHA256(_ bytes: UnsafeRawBufferPointer) -> [UInt8] {
    Array(SHA256.hash(data: bytes))
}
