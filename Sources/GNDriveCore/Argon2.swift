// Argon2id password hashing — wraps the vendored C reference implementation.
// Wire format matches alexedwards/argon2id used by the Go version:
//   $argon2id$v=19$m=65536,t=3,p=4$<salt_b64_no_pad>$<hash_b64_no_pad>
import Foundation
import CArgon2

public enum Argon2 {
    public static let memory: UInt32 = 64 * 1024 // 64MB
    public static let iterations: UInt32 = 3
    public static let parallelism: UInt32 = 4
    public static let keyLength: Int = 32
    public static let saltLength: Int = 32

    public enum Argon2Error: Error {
        case hashFailed(Int32)
        case invalidFormat
    }

    /// Derive a raw key (used for AES-256 encryption key).
    public static func deriveKey(password: String, salt: Data) -> Data {
        var out = Data(count: keyLength)
        password.withCString { pwdPtr in
            salt.withUnsafeBytes { saltPtr in
                out.withUnsafeMutableBytes { outPtr in
                    argon2id_hash_raw(
                        iterations, memory, parallelism,
                        pwdPtr, password.utf8.count,
                        saltPtr.baseAddress!, salt.count,
                        outPtr.baseAddress!, keyLength
                    )
                }
            }
        }
        return out
    }

    /// Create an encoded hash string ($argon2id$v=19$m=...$salt$hash).
    public static func createHash(password: String) throws -> String {
        var salt = Data(count: saltLength)
        salt.withUnsafeMutableBytes { ptr in
            _ = SecRandomCopyBytes(kSecRandomDefault, saltLength, ptr.baseAddress!)
        }
        let encodedLen = 256
        var encoded = [CChar](repeating: 0, count: encodedLen)
        let rc: Int32 = password.withCString { pwdPtr in
            salt.withUnsafeBytes { saltPtr in
                argon2id_hash_encoded(
                    iterations, memory, parallelism,
                    pwdPtr, password.utf8.count,
                    saltPtr.baseAddress!, salt.count,
                    keyLength,
                    &encoded, encodedLen
                )
            }
        }
        guard rc == ARGON2_OK.rawValue else { throw Argon2Error.hashFailed(rc) }
        return String(cString: encoded)
    }

    /// Verify a password against an encoded hash. Accepts both the
    /// "$argon2id$..." (6-part) and legacy "argon2id$..." (5-part) formats.
    public static func verify(password: String, encoded: String) -> Bool {
        if encoded.hasPrefix("$argon2id$") {
            var pwdc = Array(password.utf8) + [0]
            return encoded.withCString { encPtr in
                argon2id_verify(encPtr, pwdc, password.utf8.count) == ARGON2_OK.rawValue
            }
        }
        // Legacy 5-part format: argon2id$v=19$m=...,t=..,p=..$salt$hash
        let parts = encoded.split(separator: "$", omittingEmptySubsequences: false)
        guard parts.count == 5,
              let salt = Data(base64Raw: String(parts[3])),
              let stored = Data(base64Raw: String(parts[4])) else {
            return false
        }
        var out = Data(count: stored.count)
        let rc: Int32 = password.withCString { pwdPtr in
            salt.withUnsafeBytes { saltPtr in
                out.withUnsafeMutableBytes { outPtr in
                    argon2id_hash_raw(
                        iterations, memory, parallelism,
                        pwdPtr, password.utf8.count,
                        saltPtr.baseAddress!, salt.count,
                        outPtr.baseAddress!, stored.count
                    )
                }
            }
        }
        guard rc == ARGON2_OK.rawValue else { return false }
        // Constant-time compare
        var diff: UInt8 = 0
        for i in 0..<stored.count { diff |= stored[i] ^ out[i] }
        return diff == 0
    }

    /// Extract the salt from an encoded hash (either format).
    public static func extractSalt(encoded: String) throws -> Data {
        if encoded.hasPrefix("$argon2id$") {
            let parts = encoded.split(separator: "$", omittingEmptySubsequences: false)
            guard parts.count == 6, let salt = Data(base64Raw: String(parts[4])) else {
                throw Argon2Error.invalidFormat
            }
            return salt
        }
        let parts = encoded.split(separator: "$", omittingEmptySubsequences: false)
        guard parts.count == 5, let salt = Data(base64Raw: String(parts[3])) else {
            throw Argon2Error.invalidFormat
        }
        return salt
    }
}

extension Data {
    /// Base64 without padding (RawStdEncoding equivalent).
    public init?(base64Raw s: String) {
        var str = s
        let rem = str.count % 4
        if rem != 0 { str += String(repeating: "=", count: 4 - rem) }
        self.init(base64Encoded: str)
    }
}
