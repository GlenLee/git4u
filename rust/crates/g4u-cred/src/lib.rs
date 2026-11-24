//! g4u-cred — 地址级凭证元数据与 OS Keychain 存储。
//!
//! 密文只写入系统钥匙串；`credentials.json` 只保存地址、模式、用户名等元数据。
//! 网络命令通过一次性 askpass 环境变量注入，不写 `.gitconfig`。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use g4u_model::{CredentialEntry, CredentialInput, CredentialMode, G4uError};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use url::Url;

const SERVICE: &str = "dev.git4u.credential";

#[derive(Debug, Default, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default)]
    entries: Vec<CredentialEntry>,
}

pub struct CredentialStore {
    state_dir: PathBuf,
    entries: Mutex<Vec<CredentialEntry>>,
}

/// 匹配到的一条凭证；`password` 只在本次网络命令内使用。
pub struct MatchedCredential {
    pub id: String,
    pub username: String,
    pub password: String,
}

impl CredentialStore {
    pub fn new(state_dir: PathBuf) -> Self {
        let entries = load_entries(&state_dir);
        Self {
            state_dir,
            entries: Mutex::new(entries),
        }
    }

    pub fn list(&self) -> Vec<CredentialEntry> {
        let mut entries = self.entries.lock().unwrap().clone();
        entries.sort_by(|a, b| a.url.cmp(&b.url).then(a.username.cmp(&b.username)));
        entries
    }

    pub fn get(&self, id: &str) -> Option<CredentialEntry> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    pub fn save(&self, input: CredentialInput) -> Result<CredentialEntry, G4uError> {
        let url = normalize_url(&input.url)?;
        if input.username.trim().is_empty() {
            return Err(invalid("用户名不能为空。"));
        }
        let id = input
            .id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| entry_id(&url, input.username.trim(), input.mode));

        let existing = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .cloned();

        if existing.is_none() && input.password.as_deref().unwrap_or("").is_empty() {
            return Err(invalid("新增凭证必须填写密码或 Access Token。"));
        }
        if let Some(password) = input.password.as_deref().filter(|p| !p.is_empty()) {
            let keyring = Entry::new(SERVICE, &id)
                .map_err(|err| keychain_error(format!("无法打开系统钥匙串: {err}")))?;
            keyring
                .set_password(password)
                .map_err(|err| keychain_error(format!("写入系统钥匙串失败: {err}")))?;
        }

        let entry = CredentialEntry {
            id: id.clone(),
            url,
            mode: input.mode,
            username: input.username.trim().to_string(),
            last_used: existing.as_ref().and_then(|entry| entry.last_used),
            status: if input.password.as_deref().unwrap_or("").is_empty() {
                existing
                    .as_ref()
                    .map(|entry| entry.status.clone())
                    .unwrap_or_else(|| "unverified".into())
            } else {
                "unverified".into()
            },
        };

        {
            let mut entries = self.entries.lock().unwrap();
            if let Some(slot) = entries.iter_mut().find(|item| item.id == id) {
                *slot = entry.clone();
            } else {
                entries.push(entry.clone());
            }
            self.persist(&entries)?;
        }
        Ok(entry)
    }

    pub fn delete(&self, id: &str) -> Result<(), G4uError> {
        {
            let mut entries = self.entries.lock().unwrap();
            entries.retain(|entry| entry.id != id);
            self.persist(&entries)?;
        }
        if let Ok(keyring) = Entry::new(SERVICE, id) {
            let _ = keyring.delete_credential();
        }
        Ok(())
    }

    pub fn matched(&self, remote_url: &str) -> Result<Option<MatchedCredential>, G4uError> {
        let Some(target) = parse_url(remote_url) else {
            return Ok(None);
        };
        let entries = self.entries.lock().unwrap();
        let mut best: Option<&CredentialEntry> = None;
        for entry in entries.iter() {
            let Some(candidate) = parse_url(&entry.url) else {
                continue;
            };
            if candidate.origin != target.origin {
                continue;
            }
            if !candidate.path.is_empty()
                && target.path != candidate.path
                && !target.path.starts_with(&format!("{}/", candidate.path))
            {
                continue;
            }
            let better = best
                .map(|current| candidate.path.len() > parse_path_len(&current.url))
                .unwrap_or(true);
            if better {
                best = Some(entry);
            }
        }
        let Some(entry) = best else {
            return Ok(None);
        };
        let keyring = Entry::new(SERVICE, &entry.id)
            .map_err(|err| keychain_error(format!("无法打开系统钥匙串: {err}")))?;
        let password = keyring
            .get_password()
            .map_err(|err| keychain_error(format!("读取系统钥匙串失败: {err}")))?;
        Ok(Some(MatchedCredential {
            id: entry.id.clone(),
            username: entry.username.clone(),
            password,
        }))
    }

    pub fn secret_for(&self, id: &str) -> Result<MatchedCredential, G4uError> {
        let entry = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| invalid("凭证不存在或已被删除。"))?;
        let keyring = Entry::new(SERVICE, &entry.id)
            .map_err(|err| keychain_error(format!("无法打开系统钥匙串: {err}")))?;
        let password = keyring
            .get_password()
            .map_err(|err| keychain_error(format!("读取系统钥匙串失败: {err}")))?;
        Ok(MatchedCredential {
            id: entry.id,
            username: entry.username,
            password,
        })
    }

    pub fn mark_used(&self, id: &str) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
            entry.last_used = Some(now_secs());
            entry.status = "ok".into();
            let _ = self.persist(&entries);
        }
    }

    pub fn mark_failed(&self, id: &str, status: &str) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
            entry.status = status.to_string();
            let _ = self.persist(&entries);
        }
    }

    fn persist(&self, entries: &[CredentialEntry]) -> Result<(), G4uError> {
        fs::create_dir_all(&self.state_dir)
            .map_err(|err| G4uError::internal(format!("创建数据目录失败: {err}")))?;
        let json = serde_json::to_string_pretty(&CredentialFile {
            entries: entries.to_vec(),
        })
        .map_err(|err| G4uError::internal(format!("序列化凭证失败: {err}")))?;
        let file = self.state_dir.join("credentials.json");
        let tmp = self.state_dir.join("credentials.json.tmp");
        fs::write(&tmp, json).map_err(|err| G4uError::internal(format!("写入凭证失败: {err}")))?;
        fs::rename(&tmp, &file)
            .map_err(|err| G4uError::internal(format!("替换凭证文件失败: {err}")))
    }
}

fn load_entries(state_dir: &Path) -> Vec<CredentialEntry> {
    let Ok(text) = fs::read_to_string(state_dir.join("credentials.json")) else {
        return Vec::new();
    };
    serde_json::from_str::<CredentialFile>(&text)
        .map(|file| file.entries)
        .unwrap_or_default()
}

fn invalid(message: &str) -> G4uError {
    G4uError::new(g4u_model::ErrorKind::CommandFailed, message)
}

fn keychain_error(cause: String) -> G4uError {
    G4uError::new(
        g4u_model::ErrorKind::CommandFailed,
        "系统钥匙串不可用，凭证未保存。",
    )
    .with_cause(cause)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn entry_id(url: &str, username: &str, mode: CredentialMode) -> String {
    let mode = match mode {
        CredentialMode::Password => "p",
        CredentialMode::Token => "t",
    };
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!("{url}\n{username}\n{mode}").as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("c{hash:016x}")
}

fn normalize_url(raw: &str) -> Result<String, G4uError> {
    let parsed = Url::parse(raw.trim())
        .map_err(|_| invalid("地址格式不正确，示例：https://github.com/team-a"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid(
            "只支持 http / https 地址；SSH 继续沿用 ~/.ssh/config。",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid("地址中不能包含用户名或密码。"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(invalid("地址不能包含查询参数或片段。"));
    }
    let mut path = parsed.path().trim_end_matches('/').to_string();
    if path.ends_with(".git") {
        path.truncate(path.len() - 4);
    }
    let origin = parsed.origin().ascii_serialization();
    Ok(if path.is_empty() {
        origin
    } else {
        format!("{origin}{path}")
    })
}

struct ParsedUrl {
    origin: String,
    path: String,
}

fn parse_url(raw: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(raw.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let mut path = parsed.path().trim_end_matches('/').to_string();
    if path.ends_with(".git") {
        path.truncate(path.len() - 4);
    }
    Some(ParsedUrl {
        origin: parsed.origin().ascii_serialization(),
        path,
    })
}

fn parse_path_len(raw: &str) -> usize {
    parse_url(raw).map(|parsed| parsed.path.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_longest_path_and_host() {
        assert_eq!(
            normalize_url("https://github.com/git4u/api.git").unwrap(),
            "https://github.com/git4u/api"
        );
        let target = parse_url("https://github.com/git4u/api.git").unwrap();
        assert_eq!(target.origin, "https://github.com");
        assert_eq!(target.path, "/git4u/api");
        assert_eq!(parse_path_len("https://github.com/git4u"), 6);
        assert!(target.path.starts_with("/git4u/"));
        assert!(parse_url("git@github.com:git4u/api.git").is_none());
    }

    #[test]
    #[ignore = "requires an OS keychain"]
    fn keyring_roundtrip() {
        let id = format!("g4u-test-{}", std::process::id());
        let entry = Entry::new(SERVICE, &id).unwrap();
        entry.set_password("secret").unwrap();
        assert_eq!(entry.get_password().unwrap(), "secret");
        entry.delete_credential().unwrap();
    }
}
