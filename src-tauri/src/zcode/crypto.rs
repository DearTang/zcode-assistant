//! zcode credentials.json 的 enc:v1 解密
//! 算法：AES-256-GCM，key = sha256(secret)
//! 格式：enc:v1:<nonce_b64url>.<authTag_b64url>.<cipher_b64url>
//! secret = env ZCODE_CREDENTIAL_SECRET，否则
//!         zcode-credential-fallback:{platform}:{homedir}:{username}
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};

/// 推导 secret（与 zcode 客户端一致，机器+用户绑定）
fn derive_secret() -> String {
    if let Ok(s) = std::env::var("ZCODE_CREDENTIAL_SECRET") {
        return s;
    }
    let platform = if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let homedir = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    format!("zcode-credential-fallback:{}:{}:{}", platform, homedir, username)
}

fn derive_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(derive_secret().as_bytes());
    let out = hasher.finalize();
    let mut k = [0u8; 32];
    k.copy_from_slice(&out);
    k
}

/// base64url 解码（兼容有无 padding）
fn b64url_decode(s: &str) -> Result<Vec<u8>> {
    general_purpose::URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .or_else(|_| general_purpose::URL_SAFE.decode(s.as_bytes()))
        .map_err(|e| anyhow!("base64 解码失败: {e}"))
}

/// 判断是否为加密字符串
pub fn is_encrypted(s: &str) -> bool {
    s.starts_with("enc:v1:")
}

/// 解密一条 enc:v1 字符串，返回明文
pub fn decrypt(enc: &str) -> Result<String> {
    let body = enc
        .strip_prefix("enc:v1:")
        .ok_or_else(|| anyhow!("不是 enc:v1 格式"))?;
    let parts: Vec<&str> = body.splitn(3, '.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("enc:v1 结构异常"));
    }
    let nonce = b64url_decode(parts[0])?;
    let tag = b64url_decode(parts[1])?;
    let mut cipher = b64url_decode(parts[2])?;
    // aes-gcm crate 要求 ciphertext 末尾带 16 字节 tag
    cipher.extend_from_slice(&tag);

    let cipher_obj = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derive_key()));
    let nonce_obj = Nonce::from_slice(&nonce);
    let pt = cipher_obj
        .decrypt(nonce_obj, Payload { msg: &cipher, aad: &[] })
        .map_err(|e| anyhow!("AES-256-GCM 解密失败: {e}"))?;
    String::from_utf8(pt).map_err(|e| anyhow!("明文非 UTF-8: {e}"))
}

/// 从 credentials.json 的 Value 中取出并解密 zcodejwttoken
pub fn read_zcode_jwt_token(credentials: &serde_json::Value) -> Result<String> {
    let raw = credentials
        .get("zcodejwttoken")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("credentials.json 中无 zcodejwttoken"))?;
    if is_encrypted(raw) {
        decrypt(raw)
    } else {
        Ok(raw.to_string())
    }
}

/// 解码 JWT 的 payload（不验签），用于账号指纹 / 邮箱
pub fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| general_purpose::URL_SAFE.decode(parts[1]))
        .ok()?;
    serde_json::from_slice(&payload).ok()
}
