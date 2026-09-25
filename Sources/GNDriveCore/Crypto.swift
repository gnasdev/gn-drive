// AES-256-GCM file encryption — port of auth encryptFile/decryptFile.
// Wire format: [12-byte nonce][AES-256-GCM ciphertext || 16-byte tag].
// This matches CryptoKit's SealedBox combined representation byte-for-byte.
import Foundation
import CryptoKit

public enum CryptoError: Error {
    case decryptionFailed
}

public enum FileCrypto {
    /// Encrypt file at src to dst (src + ".enc" in the Go code).
    public static func encryptFile(src: String, dst: String, key: Data) throws {
        let plain = try Data(contentsOf: URL(fileURLWithPath: src))
        let nonce = AES.GCM.Nonce()
        let sealed = try AES.GCM.seal(plain, using: SymmetricKey(data: key), nonce: nonce)
        guard let combined = sealed.combined else {
            throw CryptoError.decryptionFailed
        }
        try combined.write(to: URL(fileURLWithPath: dst), options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: dst)
    }

    /// Decrypt .enc file at src to dst.
    public static func decryptFile(src: String, dst: String, key: Data) throws {
        let blob = try Data(contentsOf: URL(fileURLWithPath: src))
        guard blob.count > 12 + 16 else { throw CryptoError.decryptionFailed }
        let box = try AES.GCM.SealedBox(combined: blob)
        let plain = try AES.GCM.open(box, using: SymmetricKey(data: key))
        try plain.write(to: URL(fileURLWithPath: dst), options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: dst)
    }
}

func zeroBytes(_ data: inout Data?) {
    guard var d = data else { return }
    d.withUnsafeMutableBytes { ptr in
        memset_s(ptr.baseAddress, ptr.count, 0, ptr.count)
    }
    data = nil
}
