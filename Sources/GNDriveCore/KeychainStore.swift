// macOS Keychain-backed secret store — port of internal/securestore.
import Foundation
import Security

public enum SecureStoreError: Error {
    case notFound
    case osStatus(OSStatus)
}

public protocol SecureStore: Sendable {
    func get(account: String) throws -> Data
    func set(account: String, value: Data) throws
    func delete(account: String) throws
}

/// Persists opaque byte values in the current user's login keychain.
/// Values are base64-encoded (same as the Go go-keyring adapter).
public final class KeychainStore: SecureStore {
    private let service: String

    public init(service: String) {
        self.service = service
    }

    public func get(account: String) throws -> Data {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { throw SecureStoreError.notFound }
        guard status == errSecSuccess, let data = item as? Data else {
            throw SecureStoreError.osStatus(status)
        }
        return data
    }

    public func set(account: String, value: Data) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let attrs: [String: Any] = [kSecValueData as String: value]
        var status = SecItemUpdate(query as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var add = query
            add[kSecValueData as String] = value
            status = SecItemAdd(add as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw SecureStoreError.osStatus(status) }
    }

    public func delete(account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        if status == errSecItemNotFound { return }
        guard status == errSecSuccess else { throw SecureStoreError.osStatus(status) }
    }
}

/// In-memory store for tests.
public final class MemorySecureStore: SecureStore, @unchecked Sendable {
    private var dict: [String: Data] = [:]
    private let lock = NSLock()
    public init() {}
    public func get(account: String) throws -> Data {
        lock.lock(); defer { lock.unlock() }
        guard let v = dict[account] else { throw SecureStoreError.notFound }
        return v
    }
    public func set(account: String, value: Data) throws {
        lock.lock(); dict[account] = value; lock.unlock()
    }
    public func delete(account: String) throws {
        lock.lock(); dict.removeValue(forKey: account); lock.unlock()
    }
}
