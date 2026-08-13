// 诊断 zcodejwttoken 解密：用 Node 复刻 zcode 客户端的 zcodeCrypto 逻辑，
// 验证 secret 派生与 AES-256-GCM 解密是否成立。只输出诊断信息，不打印 token 明文。
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";

const credPath = path.join(os.homedir(), ".zcode", "v2", "credentials.json");
if (!fs.existsSync(credPath)) {
  console.log("NO credentials.json at", credPath);
  process.exit(0);
}
const cred = JSON.parse(fs.readFileSync(credPath, "utf8"));
const enc = cred["zcodejwttoken"] ?? "";
console.log("has zcodejwttoken:", !!enc);
console.log("enc prefix:", JSON.stringify(enc.slice(0, 7)));

const platform = os.platform();
const homedir = os.homedir();
const username = os.userInfo().username;
const secretSrc = process.env.ZCODE_CREDENTIAL_SECRET ? "env" : "fallback";
const secret =
  process.env.ZCODE_CREDENTIAL_SECRET ||
  `zcode-credential-fallback:${platform}:${homedir}:${username}`;
console.log("secret src:", secretSrc);
console.log("platform:", platform);
console.log("username:", username);
console.log("homedir:", homedir);
console.log("secret len:", secret.length);

if (enc.startsWith("enc:v1:")) {
  const body = enc.slice(7);
  const parts = body.split(".");
  console.log("parts count:", parts.length);
  const nonce = Buffer.from(parts[0], "base64url");
  const tag = Buffer.from(parts[1], "base64url");
  const cipher = Buffer.from(parts[2], "base64url");
  console.log("nonce/tag/cipher bytes:", nonce.length, tag.length, cipher.length);
  const key = crypto.createHash("sha256").update(secret).digest();
  try {
    const dec = crypto.createDecipheriv("aes-256-gcm", key, nonce);
    dec.setAuthTag(tag);
    let out = dec.update(cipher, null, "utf8");
    out += dec.final("utf8");
    console.log("DECRYPT: OK, len", out.length, "looks-jwt:", out.startsWith("ey"));
  } catch (e) {
    console.log("DECRYPT: FAIL -", e.message);
  }
} else {
  console.log("not enc:v1 format (plaintext?), len:", enc.length);
}
