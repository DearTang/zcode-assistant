//! ZCode app.asar 侵入式美化：自实现的 asar 解包 / 重打包 / 备份 / 还原。
//!
//! 为什么自实现而不用 `asar` crate：该 crate 的 `write_file` 第三个参数是
//! `executable`（不是 unpacked），它把**所有文件都写入 payload**，完全不支持
//! `unpacked: true` 标记。而 ZCode 的 node-pty / ssh2 等 native 模块是 unpacked 的
//! （数据在 app.asar.unpacked 目录，asar 内只有 JSON 条目）。若把它们打包进 payload，
//! Electron 无法正确加载 native 模块，ZCode 终端会坏。因此必须自实现 pack，精确复现
//! `unpacked` 标记。
//!
//! asar 格式（已对真实 ZCode app.asar 验证）：
//!   [u32 = 4][u32 = aligned+8][u32 = aligned+4][u32 = json_len]
//!   [json 字节，补齐到 4 字节对齐]
//!   [payload：文件内容顺序拼接；unpacked 文件不占 payload]
//! JSON 头：
//!   目录 {"files": {...}}
//!   packed 文件 {"size":N,"offset":"<str>","integrity":{...}}
//!   unpacked 文件 {"size":N,"unpacked":true,"integrity":{...}}   （无 offset）
//!   integrity {"algorithm":"SHA256","hash":"<hex>","blockSize":4194304,"blocks":["<hex>",...]}
//!
//! 完整性校验已确认关闭（Electron FUSE[4]=off，exe 内无 ElectronAsarIntegrity），
//! 重打包后 ZCode 能正常启动、不破坏 exe 签名。
//!
//! 两条改造路径：
//! - `extract` + `pack`：全量解包/重打包（离线验证用）。对 284MB 的真实包意味着
//!   几千个小文件的读写删 + 全量 SHA256，秒级到分钟级。
//! - `patch`：原地补丁（apply 实际走的快速路径）。只改 JSON 头里被触及的条目，
//!   新内容追加到 payload 尾部，原 payload 字节顺序照抄——一次顺序拷贝搞定。
use crate::zcode::paths;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::HashSet;
use std::fs;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// app.asar 的绝对路径：由 ZCode.exe 安装目录推导（<安装目录>/resources/app.asar）。
pub fn asar_path() -> Result<PathBuf> {
    let exe = paths::find_zcode_exe().ok_or_else(|| anyhow!("未找到 ZCode.exe"))?;
    PathBuf::from(exe)
        .parent()
        .map(|p| p.join("resources").join("app.asar"))
        .ok_or_else(|| anyhow!("无法推导 ZCode 安装目录"))
}

/// app.asar.unpacked 目录（native 模块真实内容所在）。
#[allow(dead_code)]
pub fn unpacked_dir() -> Result<PathBuf> {
    Ok(asar_path()?
        .parent()
        .ok_or_else(|| anyhow!("app.asar 无父目录"))?
        .join("app.asar.unpacked"))
}

/// 备份目录：zcode-assistant 自身 app data（不放 ZCode 目录，避免被卸载/更新清理）。
pub fn backup_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| anyhow!("无法定位 AppData 目录"))?;
    Ok(base
        .join("com.zcode-assistant.app")
        .join("beautify"))
}

/// 原始 app.asar 备份路径。
pub fn origin_backup_path() -> Result<PathBuf> {
    Ok(backup_dir()?.join("app.asar.origin"))
}

// ───────────────────────── 解包（离线验证 / 测试用）─────────────────────────

/// 把 app.asar 解包到 dest 目录。返回 unpacked 文件的相对路径集合（正斜杠分隔）。
///
/// - packed 文件：按 offset 从 asar payload 流式读出。
/// - unpacked 文件：从 app.asar.unpacked 目录复制（其内容不在 asar 内）。
#[cfg(test)]
pub fn extract(asar: &Path, dest: &Path) -> Result<HashSet<String>> {
    fs::create_dir_all(dest).with_context(|| format!("创建解包目录失败: {}", dest.display()))?;
    let unpacked_root = asar
        .parent()
        .map(|p| p.join("app.asar.unpacked"))
        .unwrap_or_else(|| PathBuf::from("app.asar.unpacked"));

    let mut src = fs::File::open(asar).with_context(|| format!("打开 asar 失败: {}", asar.display()))?;
    let (tree, payload_base) = read_header(&mut src)?;
    let mut unpacked = HashSet::new();
    let root_files = tree
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("asar JSON 头缺少 files 字段"))?;
    extract_dir(
        &mut src,
        root_files,
        dest,
        dest,
        payload_base,
        &unpacked_root,
        &mut unpacked,
    )?;
    Ok(unpacked)
}

/// 读取 asar 头部，返回 (JSON 树, payload 基准偏移)。
fn read_header(src: &mut fs::File) -> Result<(Value, u64)> {
    let mut buf12 = [0u8; 12];
    src.read_exact(&mut buf12)?;
    // bytes 0-3 = 4（头大小标记），4-7 / 8-11 = pickle 长度字段（重打包时复算，读取忽略）
    let mut b4 = [0u8; 4];
    src.read_exact(&mut b4)?;
    let json_len = u32::from_le_bytes(b4) as usize;
    let mut json_bytes = vec![0u8; json_len];
    src.read_exact(&mut json_bytes)?;
    let tree: Value = serde_json::from_slice(&json_bytes).context("asar JSON 头解析失败")?;
    let padding = (4 - (json_len % 4)) % 4;
    let payload_base = (16 + json_len + padding) as u64;
    let _ = src.seek(SeekFrom::Start(0)); // 还原读取位置由调用方按 offset seek
    Ok((tree, payload_base))
}

/// 按内部路径（如 "out/renderer/index.html"）在 JSON 头树中定位节点。
fn navigate<'a>(tree: &'a Value, inner_path: &str) -> Option<&'a Value> {
    let mut cur = tree;
    for part in inner_path.split('/') {
        cur = cur.get("files")?.get(part)?;
    }
    Some(cur)
}

/// 从 asar 读取单个内部文件内容（不解包全部，用于状态检测/版本探测）。
/// unpacked 文件从 app.asar.unpacked 目录读取。
pub fn read_file(asar: &Path, inner_path: &str) -> Result<Vec<u8>> {
    let mut f = fs::File::open(asar)?;
    let (tree, payload_base) = read_header(&mut f)?;
    let node = navigate(&tree, inner_path)
        .ok_or_else(|| anyhow!("asar 内未找到 {}", inner_path))?;
    let size = node
        .get("size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("{} 缺少 size", inner_path))?;
    let is_unpacked = node
        .get("unpacked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_unpacked {
        let p = asar
            .parent()
            .ok_or_else(|| anyhow!("app.asar 无父目录"))?
            .join("app.asar.unpacked")
            .join(inner_path);
        return Ok(fs::read(&p)?);
    }
    let offset = node
        .get("offset")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("{} 缺少 offset", inner_path))?;
    f.seek(SeekFrom::Start(payload_base + offset))?;
    let mut buf = vec![0u8; size as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// 读取 ZCode 版本号（asar 根 package.json 的 version 字段）。
pub fn read_zcode_version(asar: &Path) -> Option<String> {
    let data = read_file(asar, "package.json").ok()?;
    let v: Value = serde_json::from_slice(&data).ok()?;
    v.get("version").and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// 列出 asar 内某目录下的直接子项名（文件与子目录）。目录不存在返回 Err。
pub fn list_dir(asar: &Path, inner_dir: &str) -> Result<Vec<String>> {
    let mut f = fs::File::open(asar)?;
    let (tree, _) = read_header(&mut f)?;
    let node = navigate(&tree, inner_dir)
        .ok_or_else(|| anyhow!("asar 内未找到目录 {}", inner_dir))?;
    let files = node
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("asar 内 {} 不是目录", inner_dir))?;
    Ok(files.keys().cloned().collect())
}

/// navigate 的可变版本（用于就地修改 JSON 头条目）。
fn navigate_mut<'a>(tree: &'a mut Value, inner_path: &str) -> Option<&'a mut Value> {
    let mut cur = tree;
    for part in inner_path.split('/') {
        cur = cur.get_mut("files")?.get_mut(part)?;
    }
    Some(cur)
}

/// 从 JSON 头树中删除一个文件条目（目录节点，含最后一段）下的键。不存在则无操作。
fn remove_entry(tree: &mut Value, inner_path: &str) {
    let parts: Vec<&str> = inner_path.split('/').collect();
    let (dirs, name) = parts.split_at(parts.len() - 1);
    let mut cur = tree;
    for p in dirs {
        cur = match cur.get_mut("files").and_then(|f| f.get_mut(*p)) {
            Some(c) => c,
            None => return,
        };
    }
    if let Some(files) = cur.get_mut("files").and_then(|v| v.as_object_mut()) {
        files.remove(name[0]);
    }
}

// ───────────────────────── 原地补丁（快速路径）─────────────────────────

/// 单个 asar 内部文件的修改：写入/替换内容，或删除条目。
#[derive(Clone)]
pub enum FileMod {
    /// 替换或新增文件（内容追加到 payload 尾部，原条目指向新位置）。
    Write(Vec<u8>),
    /// 删除条目（原字节留在 payload 中成为无引用死空间，无害）。
    Remove,
}

/// 就地补丁 asar：不解包、不整包重打包，只替换 / 新增 / 删除少量内部文件。
///
/// 原理：JSON 头里的 offset 是相对 payload 起点的**相对偏移**。未修改条目的
/// offset/integrity 原样保留，原 payload 字节顺序照抄（相对偏移不变即有效）；
/// 被替换 / 新增文件的内容追加到 payload 尾部，条目改指新位置；被替换文件的旧
/// 字节成为无引用死空间（每次 apply 累积几十 KB，无害；还原永远从备份恢复）。
/// IO 从「全量解包 + 全量重打包」降为一次顺序拷贝 + 少量哈希。
pub fn patch(asar: &Path, mods: &[(String, FileMod)], out: &Path) -> Result<()> {
    let mut src =
        fs::File::open(asar).with_context(|| format!("打开 asar 失败: {}", asar.display()))?;
    let file_len = src.metadata()?.len();
    let (mut tree, payload_base) = read_header(&mut src)?;
    let old_payload_len = file_len
        .checked_sub(payload_base)
        .ok_or_else(|| anyhow!("asar 头长度异常：payload 起点超过文件长度"))?;

    let mut append_offset: u64 = old_payload_len;
    let mut appended: Vec<u8> = Vec::new();
    for (inner_path, m) in mods {
        let FileMod::Write(data) = m else {
            remove_entry(&mut tree, inner_path);
            continue;
        };
        let size = data.len() as u64;
        let (hash, blocks) = sha256_and_blocks(data);
        let integrity = integrity_json(&hash, &blocks);
        let offset_str = append_offset.to_string();
        append_offset += size;
        appended.extend_from_slice(data);

        if let Some(node) = navigate(&tree, inner_path) {
            if node.get("files").is_some() {
                return Err(anyhow!("asar 内 {} 是目录，无法写入文件", inner_path));
            }
            // 已有条目：只改 size/offset/integrity，executable 等其他字段原样保留
            let obj = navigate_mut(&mut tree, inner_path)
                .and_then(|n| n.as_object_mut())
                .expect("navigate/navigate_mut 结果不一致");
            obj.remove("unpacked");
            obj.insert("size".to_string(), Value::from(size));
            obj.insert("offset".to_string(), Value::String(offset_str));
            obj.insert("integrity".to_string(), integrity);
        } else {
            // 新文件：沿路径创建目录节点后插入叶子
            let root_files = tree
                .get_mut("files")
                .and_then(|v| v.as_object_mut())
                .ok_or_else(|| anyhow!("asar JSON 头缺少 files 字段"))?;
            let mut entry = Map::new();
            entry.insert("size".to_string(), Value::from(size));
            entry.insert("offset".to_string(), Value::String(offset_str));
            entry.insert("integrity".to_string(), integrity);
            insert_leaf(root_files, inner_path, Value::Object(entry));
        }
    }

    // 头部序列化 + 对齐（与 pack 一致；serde_json 开了 preserve_order，键序不变）
    let json_bytes = serde_json::to_vec(&tree).context("序列化 asar JSON 头失败")?;
    let json_size = json_bytes.len() as u32;
    let pad = (4 - (json_size % 4)) % 4;
    let aligned = json_size + pad;

    let mut out_f = BufWriter::with_capacity(
        4 * 1024 * 1024,
        fs::File::create(out).with_context(|| format!("创建输出失败: {}", out.display()))?,
    );
    out_f.write_all(&4u32.to_le_bytes())?; // 头大小标记
    out_f.write_all(&((aligned + 8) as u32).to_le_bytes())?;
    out_f.write_all(&((aligned + 4) as u32).to_le_bytes())?;
    out_f.write_all(&json_size.to_le_bytes())?;
    out_f.write_all(&json_bytes)?;
    if pad > 0 {
        out_f.write_all(&vec![0u8; pad as usize])?;
    }
    // 原 payload 原样照抄（未修改文件的字节与相对偏移全部保持不变）
    src.seek(SeekFrom::Start(payload_base))?;
    std::io::copy(&mut src.take(old_payload_len), &mut out_f)?;
    // 追加区：本次写入/替换的文件内容
    out_f.write_all(&appended)?;
    out_f.flush()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn extract_dir(
    src: &mut fs::File,
    node: &Map<String, Value>,
    dest_root: &Path,
    current: &Path,
    payload_base: u64,
    unpacked_root: &Path,
    unpacked: &mut HashSet<String>,
) -> Result<()> {
    for (name, child) in node.iter() {
        let target = current.join(name);
        if let Some(sub) = child.get("files").and_then(|v| v.as_object()) {
            // 目录
            fs::create_dir_all(&target)?;
            extract_dir(src, sub, dest_root, &target, payload_base, unpacked_root, unpacked)?;
        } else {
            // 文件
            let rel = target
                .strip_prefix(dest_root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            let size = child
                .get("size")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow!("文件 {} 缺少 size", rel))?;
            let is_unpacked = child
                .get("unpacked")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_unpacked {
                let src_unpacked = unpacked_root.join(&rel);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src_unpacked, &target)
                    .with_context(|| format!("复制 unpacked 文件失败: {}", src_unpacked.display()))?;
                unpacked.insert(rel);
            } else {
                let offset = child
                    .get("offset")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or_else(|| anyhow!("文件 {} 缺少 offset", rel))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                src.seek(SeekFrom::Start(payload_base + offset))?;
                copy_n(src, &target, size)?;
            }
        }
    }
    Ok(())
}

/// 从 src 流式复制 size 字节到 dest 文件。
#[cfg(test)]
fn copy_n(src: &mut fs::File, dest_path: &Path, size: u64) -> Result<()> {
    let mut out = fs::File::create(dest_path)?;
    let mut remaining = size;
    let mut buf = vec![0u8; 65536];
    while remaining > 0 {
        let to_read = remaining.min(buf.len() as u64) as usize;
        let n = src.read(&mut buf[..to_read])?;
        if n == 0 {
            return Err(anyhow!("读取提前结束：{}", dest_path.display()));
        }
        out.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    Ok(())
}

// ───────────────────────── 重打包（离线验证 / 测试用）─────────────────────────

/// 把 src 目录重打包为 asar。unpacked_set 中的相对路径文件会被标记为 unpacked
/// （不写入 payload，内容由 app.asar.unpacked 目录提供）。
///
/// 两遍处理：第一遍算每个文件的 SHA256 + 块哈希、分配 offset、构建 JSON 头；
/// 第二遍按顺序把 packed 文件内容流式写入 payload。
#[cfg(test)]
pub fn pack(src: &Path, out: &Path, unpacked_set: &HashSet<String>) -> Result<()> {
    // 1. 收集全部文件（相对路径 + 绝对路径），排序保证可重现
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    walk_collect(src, Path::new(""), &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // 2. 第一遍：算哈希、分配 offset、构建 JSON 头树
    let mut root_files: Map<String, Value> = Map::new();
    let mut offset: u64 = 0;
    let mut packed_order: Vec<PathBuf> = Vec::new(); // payload 中 packed 文件的顺序
    for (rel, full) in &entries {
        let data = fs::read(full).with_context(|| format!("读取文件失败: {}", full.display()))?;
        let size = data.len() as u64;
        let (hash, blocks) = sha256_and_blocks(&data);
        let is_unpacked = unpacked_set.contains(rel);
        let mut entry = Map::new();
        entry.insert("size".to_string(), Value::from(size));
        if is_unpacked {
            entry.insert("unpacked".to_string(), Value::Bool(true));
        } else {
            entry.insert("offset".to_string(), Value::String(offset.to_string()));
            offset += size;
            packed_order.push(full.clone());
        }
        entry.insert("integrity".to_string(), integrity_json(&hash, &blocks));
        insert_leaf(&mut root_files, rel, Value::Object(entry));
        // 释放当前文件内容，避免峰值内存叠加
        drop(data);
    }
    let header = json!({ "files": Value::Object(root_files) });

    // 3. 序列化 JSON + 对齐
    let json_bytes = serde_json::to_vec(&header).context("序列化 asar JSON 头失败")?;
    let json_size = json_bytes.len() as u32;
    let pad = (4 - (json_size % 4)) % 4;
    let aligned = json_size + pad;

    // 4. 写出：[16字节头][对齐 JSON][payload]
    let mut out_f = fs::File::create(out).with_context(|| format!("创建输出失败: {}", out.display()))?;
    out_f.write_all(&4u32.to_le_bytes())?; // 头大小标记
    out_f.write_all(&(aligned + 8).to_le_bytes())?;
    out_f.write_all(&(aligned + 4).to_le_bytes())?;
    out_f.write_all(&json_size.to_le_bytes())?;
    out_f.write_all(&json_bytes)?;
    if pad > 0 {
        out_f.write_all(&vec![0u8; pad as usize])?;
    }
    for full in &packed_order {
        let mut f = fs::File::open(full)?;
        std::io::copy(&mut f, &mut out_f)?;
    }
    out_f.flush()?;
    Ok(())
}

/// 递归收集目录下所有文件，rel 为相对 src 的正斜杠路径。
#[cfg(test)]
fn walk_collect(base: &Path, rel: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let dir = if rel.as_os_str().is_empty() {
        base.to_path_buf()
    } else {
        base.join(rel)
    };
    for entry in fs::read_dir(&dir).with_context(|| format!("读取目录失败: {}", dir.display()))? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let name = entry.file_name();
        let child_rel = if rel.as_os_str().is_empty() {
            PathBuf::from(&name)
        } else {
            rel.join(&name)
        };
        if ft.is_dir() {
            walk_collect(base, &child_rel, out)?;
        } else if ft.is_file() {
            let rel_str = child_rel.to_string_lossy().replace('\\', "/");
            out.push((rel_str, entry.path()));
        }
    }
    Ok(())
}

/// 把一个文件条目按路径层级插入 JSON 头树（root_files 代表顶层 "files" 的值）。
fn insert_leaf(root_files: &mut Map<String, Value>, rel: &str, leaf: Value) {
    let parts: Vec<&str> = rel.split('/').collect();
    insert_rec(root_files, &parts, leaf);
}

fn insert_rec(files: &mut Map<String, Value>, parts: &[&str], leaf: Value) {
    let name = parts[0];
    if parts.len() == 1 {
        files.insert(name.to_string(), leaf);
        return;
    }
    // 需要进入/创建子目录
    let child = files
        .entry(name.to_string())
        .or_insert_with(|| Value::Object({ let mut m = Map::new(); m.insert("files".to_string(), Value::Object(Map::new())); m }));
    let sub_files = child
        .get_mut("files")
        .and_then(|v| v.as_object_mut())
        .expect("目录节点结构错误");
    insert_rec(sub_files, &parts[1..], leaf);
}

/// 计算整文件 SHA256（hex）与每个 4MB 块的 SHA256（hex 数组）。
fn sha256_and_blocks(data: &[u8]) -> (String, Vec<String>) {
    let mut full = Sha256::new();
    full.update(data);
    let hash = hex_encode(&full.finalize());
    let mut blocks = Vec::new();
    for chunk in data.chunks(BLOCK_SIZE) {
        let mut h = Sha256::new();
        h.update(chunk);
        blocks.push(hex_encode(&h.finalize()));
    }
    (hash, blocks)
}

fn integrity_json(hash: &str, blocks: &[String]) -> Value {
    json!({
        "algorithm": "SHA256",
        "hash": hash,
        "blockSize": BLOCK_SIZE,
        "blocks": blocks,
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ───────────────────────── 备份 / 还原 ─────────────────────────

/// 首次美化前备份原始 app.asar（若已备份则跳过）。返回是否执行了备份。
pub fn ensure_backup() -> Result<bool> {
    let origin = origin_backup_path()?;
    if origin.exists() {
        return Ok(false);
    }
    if let Some(parent) = origin.parent() {
        fs::create_dir_all(parent)?;
    }
    let src = asar_path()?;
    fs::copy(&src, &origin)
        .with_context(|| format!("备份 app.asar 失败: {} -> {}", src.display(), origin.display()))?;
    Ok(true)
}

/// 用备份还原 app.asar。返回是否执行了还原（无备份则报错）。
pub fn restore_from_backup() -> Result<()> {
    let origin = origin_backup_path()?;
    if !origin.exists() {
        return Err(anyhow!("未找到原始备份，无法还原（{}）", origin.display()));
    }
    let dest = asar_path()?;
    fs::copy(&origin, &dest)
        .with_context(|| format!("还原 app.asar 失败: {} -> {}", origin.display(), dest.display()))?;
    Ok(())
}

// ───────────────────────── 离线验证（不碰真实 ZCode）─────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// patch 单测（合成小 asar，不依赖 ZCode）：
    /// 替换/新增/删除条目 → read_file 校验内容、未修改文件保持不变、
    /// 对已 patch 的包二次 patch（模拟重复应用）、unpacked 标记保留。
    #[test]
    fn patch_write_add_remove() {
        let work = std::env::temp_dir().join("zcode_asar_patch_test");
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).unwrap();

        // 构造源目录：index.html + assets + package.json + unpacked native 模块
        let src_dir = work.join("src");
        fs::create_dir_all(src_dir.join("out/renderer/assets")).unwrap();
        fs::create_dir_all(src_dir.join("node_modules/pty")).unwrap();
        fs::write(
            src_dir.join("out/renderer/index.html"),
            b"<html><head></head><body>hi</body></html>",
        )
        .unwrap();
        fs::write(src_dir.join("out/renderer/assets/a.css"), b"body{}").unwrap();
        fs::write(src_dir.join("package.json"), br#"{"version":"9.9.9"}"#).unwrap();
        fs::write(src_dir.join("node_modules/pty/pty.node"), b"\0binary").unwrap();

        let mut unpacked_set = HashSet::new();
        unpacked_set.insert("node_modules/pty/pty.node".to_string());
        let asar = work.join("app.asar");
        pack(&src_dir, &asar, &unpacked_set).unwrap();

        // patch：替换 index.html、新增 js、删除 a.css
        let mods = vec![
            (
                "out/renderer/index.html".to_string(),
                FileMod::Write(b"<html><head><link></head><body>new</body></html>".to_vec()),
            ),
            (
                "out/renderer/assets/zcode-custom.js".to_string(),
                FileMod::Write(b"// js".to_vec()),
            ),
            ("out/renderer/assets/a.css".to_string(), FileMod::Remove),
        ];
        let patched = work.join("patched.asar");
        patch(&asar, &mods, &patched).unwrap();

        assert_eq!(
            read_file(&patched, "out/renderer/index.html").unwrap(),
            b"<html><head><link></head><body>new</body></html>"
        );
        assert_eq!(
            read_file(&patched, "out/renderer/assets/zcode-custom.js").unwrap(),
            b"// js"
        );
        assert!(
            read_file(&patched, "out/renderer/assets/a.css").is_err(),
            "已删除的条目不应再可读"
        );
        assert_eq!(
            read_file(&patched, "package.json").unwrap(),
            br#"{"version":"9.9.9"}"#,
            "未修改文件内容应保持不变"
        );
        assert_eq!(
            read_zcode_version(&patched).as_deref(),
            Some("9.9.9"),
            "版本探测应继续工作"
        );

        // 二次 patch（模拟重复应用）：对已补丁过的包再做增量修改
        let mods2 = vec![(
            "out/renderer/assets/zcode-custom.js".to_string(),
            FileMod::Write(b"// js v2".to_vec()),
        )];
        let patched2 = work.join("patched2.asar");
        patch(&patched, &mods2, &patched2).unwrap();
        assert_eq!(
            read_file(&patched2, "out/renderer/assets/zcode-custom.js").unwrap(),
            b"// js v2"
        );
        assert_eq!(
            read_file(&patched2, "out/renderer/index.html").unwrap(),
            b"<html><head><link></head><body>new</body></html>",
            "二次 patch 不应影响前次修改"
        );

        // unpacked 标记在 patch 后保留：extract 需要 app.asar.unpacked 同级目录
        let unpacked_dir = work.join("app.asar.unpacked");
        fs::create_dir_all(unpacked_dir.join("node_modules/pty")).unwrap();
        fs::write(
            unpacked_dir.join("node_modules/pty/pty.node"),
            b"\0binary",
        )
        .unwrap();
        let dir_out = work.join("extracted");
        let unp = extract(&patched2, &dir_out).unwrap();
        assert!(
            unp.contains("node_modules/pty/pty.node"),
            "unpacked 标记丢失，native 模块会被错误打进 payload"
        );
        assert!(read_file(&patched2, "node_modules/pty/pty.node").is_ok());

        // 新增深层路径（目录节点不存在时应自动创建）
        let mods3 = vec![(
            "out/renderer/assets/newdir/deep/file.txt".to_string(),
            FileMod::Write(b"deep".to_vec()),
        )];
        let patched3 = work.join("patched3.asar");
        patch(&patched2, &mods3, &patched3).unwrap();
        assert_eq!(
            read_file(&patched3, "out/renderer/assets/newdir/deep/file.txt").unwrap(),
            b"deep"
        );

        let _ = fs::remove_dir_all(&work);
    }

    /// 基准：对真实 app.asar 副本做 patch（替换 index.html + 新增 css/js），
    /// 测量耗时并校验可读。全程在临时目录操作，绝不触碰真实 ZCode。
    /// 手动跑：`cargo test -- --ignored patch_real_asar_bench --nocapture`
    #[test]
    #[ignore]
    fn patch_real_asar_bench() {
        let real = asar_path().expect("未找到 app.asar，无法验证");
        let work = std::env::temp_dir().join("zcode_asar_patch_bench");
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work).unwrap();
        let copy = work.join("app.asar");
        println!("复制 app.asar 副本...");
        fs::copy(&real, &copy).expect("复制 app.asar 失败");

        let mods = vec![
            (
                "out/renderer/index.html".to_string(),
                FileMod::Write(b"<html><head></head><body></body></html>".to_vec()),
            ),
            (
                "out/renderer/assets/zcode-custom.css".to_string(),
                FileMod::Write(b"/* bench */".to_vec()),
            ),
            (
                "out/renderer/assets/zcode-custom.js".to_string(),
                FileMod::Write(b"// bench".to_vec()),
            ),
        ];
        let out = work.join("patched.asar");
        let t0 = std::time::Instant::now();
        patch(&copy, &mods, &out).expect("patch 失败");
        println!(
            "patch 耗时 {:?}（{} -> {} 字节）",
            t0.elapsed(),
            fs::metadata(&copy).unwrap().len(),
            fs::metadata(&out).unwrap().len()
        );
        assert!(read_file(&out, "package.json").is_ok(), "patch 后 asar 头损坏");
        assert_eq!(
            read_file(&out, "out/renderer/assets/zcode-custom.css").unwrap(),
            b"/* bench */"
        );
        let _ = fs::remove_dir_all(&work);
    }

    /// 调研用：把真实 app.asar 解包到 %TEMP%\zcode_research（只读原文件，绝不写回）。
    /// 手动跑：`cargo test -- --ignored dump_for_research`
    #[test]
    #[ignore]
    fn dump_for_research() {
        let real = asar_path().expect("未找到 app.asar");
        let dest = std::env::temp_dir().join("zcode_research");
        if dest.exists() {
            fs::remove_dir_all(&dest).unwrap();
        }
        let t0 = std::time::Instant::now();
        let unpacked = extract(&real, &dest).expect("extract 失败");
        println!(
            "解包完成 -> {}（unpacked {} 个，耗时 {:?}）",
            dest.display(),
            unpacked.len(),
            t0.elapsed()
        );
    }

    /// 对真实 app.asar 副本做 extract → pack → extract，逐文件比对内容哈希。
    /// 全程在临时目录操作，绝不触碰真实 ZCode。手动跑：`cargo test -- --ignored roundtrip`
    #[test]
    #[ignore]
    fn roundtrip_real_asar() {
        let real = asar_path().expect("未找到 app.asar，无法验证");
        let real_unpacked = real
            .parent()
            .unwrap()
            .join("app.asar.unpacked");
        // 工作目录：temp/zcode_asar_rt/
        let work = std::env::temp_dir().join("zcode_asar_rt");
        if work.exists() {
            fs::remove_dir_all(&work).unwrap();
        }
        let work_asar = work.join("app.asar");
        let work_unpacked = work.join("app.asar.unpacked");
        fs::create_dir_all(&work).unwrap();
        println!("复制 app.asar 副本...");
        fs::copy(&real, &work_asar).expect("复制 app.asar 失败");
        // 复制 unpacked 目录（extract unpacked 文件需要）
        copy_dir_all(&real_unpacked, &work_unpacked);

        // 1) 解包原始副本
        println!("解包原始副本...");
        let dir1 = work.join("dir1");
        let unpacked_set = extract(&work_asar, &dir1).expect("extract 原始失败");
        println!("解包完成，文件数（含 unpacked）：{}", count_files(&dir1));
        println!("unpacked 文件数：{}", unpacked_set.len());
        let hash1 = hash_tree(&dir1);

        // 2) 重打包（repacked.asar 必须与 app.asar.unpacked 同级，以便再次解包 unpacked 文件）
        println!("重打包...");
        let repacked = work.join("repacked.asar");
        pack(&dir1, &repacked, &unpacked_set).expect("pack 失败");
        let orig_size = fs::metadata(&work_asar).unwrap().len();
        let new_size = fs::metadata(&repacked).unwrap().len();
        println!("原始 asar: {} 字节，重打包: {} 字节", orig_size, new_size);

        // 3) 解包重打包结果
        println!("解包重打包结果...");
        let dir2 = work.join("dir2");
        let _ = extract(&repacked, &dir2).expect("extract 重打包失败");
        let hash2 = hash_tree(&dir2);

        // 4) 逐文件比对
        assert_eq!(hash1.len(), hash2.len(), "文件数量不一致");
        let mut missing = vec![];
        let mut differ = vec![];
        for (rel, h) in &hash1 {
            match hash2.get(rel) {
                Some(h2) if h2 == h => {}
                Some(_) => differ.push(rel.clone()),
                None => missing.push(rel.clone()),
            }
        }
        if !missing.is_empty() || !differ.is_empty() {
            for m in &missing {
                println!("缺失: {}", m);
            }
            for d in &differ {
                println!("内容不同: {}", d);
            }
        }
        assert!(missing.is_empty(), "重打包后缺失 {} 个文件", missing.len());
        assert!(differ.is_empty(), "重打包后 {} 个文件内容不同", differ.len());
        println!("✅ round-trip 验证通过：{} 个文件内容完全一致", hash1.len());
    }

    fn copy_dir_all(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ft = entry.file_type().unwrap();
            let dest = dst.join(entry.file_name());
            if ft.is_dir() {
                copy_dir_all(&entry.path(), &dest);
            } else {
                fs::copy(entry.path(), dest).unwrap();
            }
        }
    }

    fn count_files(dir: &Path) -> usize {
        let mut n = 0;
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let ft = entry.file_type().unwrap();
            if ft.is_dir() {
                n += count_files(&entry.path());
            } else {
                n += 1;
            }
        }
        n
    }

    /// 返回目录下所有文件的 (相对路径(正斜杠), sha256) 映射。
    fn hash_tree(dir: &Path) -> HashMap<String, String> {
        let mut map = HashMap::new();
        hash_rec(dir, dir, &mut map);
        map
    }

    fn hash_rec(base: &Path, current: &Path, map: &mut HashMap<String, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            let ft = entry.file_type().unwrap();
            if ft.is_dir() {
                hash_rec(base, &entry.path(), map);
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = fs::read(entry.path()).unwrap();
                let mut h = Sha256::new();
                h.update(&data);
                map.insert(rel, hex_encode(&h.finalize()));
            }
        }
    }
}
