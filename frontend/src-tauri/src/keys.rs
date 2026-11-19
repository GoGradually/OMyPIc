//! API 키 보관. Electron(keytar)과 같은 항목(서비스 `OMyPIc`, 계정 `openai`/`anthropic`)을 쓰므로
//! 이전 버전에서 저장한 키를 그대로 읽는다. 키 값은 로그에 남기지 않는다.

pub const SERVICE: &str = "OMyPIc";
const OPENAI: &str = "openai";
const ANTHROPIC: &str = "anthropic";

/// 키를 저장할 계정. 모르는 provider는 `openai`로 본다.
pub fn resolve_provider(provider: Option<&str>) -> &'static str {
    match provider {
        Some(ANTHROPIC) => ANTHROPIC,
        _ => OPENAI,
    }
}

/// 계정별 비밀 값 저장소. 오류는 사용자에게 보여 줄 수 있는 메시지다.
pub trait KeyStore {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    /// 없는 항목을 지우는 것은 성공이다.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// 저장된 키. 없거나 읽을 수 없으면(키체인을 쓸 수 없는 경우 포함) `None`.
pub fn get_api_key(store: &dyn KeyStore, provider: Option<&str>) -> Option<String> {
    let account = resolve_provider(provider);
    match store.get(account) {
        Ok(value) => value.filter(|key| !key.is_empty()),
        Err(err) => {
            tracing::warn!("Reading the {account} API key failed: {err}");
            None
        }
    }
}

/// 앞뒤 공백을 뗀 키를 저장한다. 비었으면 항목을 지운다.
pub fn set_api_key(
    store: &dyn KeyStore,
    key: Option<&str>,
    provider: Option<&str>,
) -> Result<(), String> {
    let account = resolve_provider(provider);
    let value = key.map(str::trim).unwrap_or_default();
    let result = if value.is_empty() {
        store.delete(account)
    } else {
        store.set(account, value)
    };
    if let Err(err) = &result {
        tracing::warn!("Saving the {account} API key failed: {err}");
    }
    result
}

/// 저장된 바이트를 문자열로 읽는다. keytar는 UTF-8로, `keyring`의 Windows 비밀번호는 UTF-16LE로
/// 저장한다. NUL 없는 UTF-8이면 그대로, 아니면 UTF-16LE로 읽는다. 둘 다 아니면 `None`.
pub fn decode_secret(bytes: &[u8]) -> Option<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if !text.contains('\0') {
            return Some(text.to_string());
        }
    }
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16(&units).ok()?;
    Some(text.trim_end_matches('\0').to_string())
}

/// 운영체제 키체인(macOS 키체인, Windows 자격 증명 관리자, Linux Secret Service).
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        // keytar는 Windows에서 대상 이름을 `<service>/<account>`로 쓴다.
        #[cfg(target_os = "windows")]
        let entry =
            keyring::Entry::new_with_target(&format!("{SERVICE}/{account}"), SERVICE, account);
        #[cfg(not(target_os = "windows"))]
        let entry = keyring::Entry::new(SERVICE, account);
        entry.map_err(|err| err.to_string())
    }
}

impl KeyStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_secret() {
            Ok(bytes) => Ok(decode_secret(&bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        // keytar와 같이 UTF-8 바이트로 저장한다(Windows에서 `set_password`는 UTF-16으로 저장한다).
        Self::entry(account)?
            .set_secret(value.as_bytes())
            .map_err(|err| err.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct FakeStore {
        entries: RefCell<HashMap<String, String>>,
        broken: bool,
    }

    impl KeyStore for FakeStore {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            if self.broken {
                return Err("no keychain".into());
            }
            Ok(self.entries.borrow().get(account).cloned())
        }

        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            if self.broken {
                return Err("no keychain".into());
            }
            self.entries
                .borrow_mut()
                .insert(account.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            if self.broken {
                return Err("no keychain".into());
            }
            self.entries.borrow_mut().remove(account);
            Ok(())
        }
    }

    #[test]
    fn provider_defaults_to_openai() {
        assert_eq!("openai", resolve_provider(None));
        assert_eq!("openai", resolve_provider(Some("x")));
        assert_eq!("openai", resolve_provider(Some("openai")));
        assert_eq!("anthropic", resolve_provider(Some("anthropic")));
    }

    #[test]
    fn set_empty_key_deletes_entry() {
        let store = FakeStore::default();
        set_api_key(&store, Some("sk-1"), Some("anthropic")).unwrap();
        set_api_key(&store, Some("   "), Some("anthropic")).unwrap();
        assert_eq!(None, store.entries.borrow().get("anthropic"));

        set_api_key(&store, Some("sk-2"), None).unwrap();
        set_api_key(&store, None, None).unwrap();
        assert!(store.entries.borrow().is_empty());
    }

    #[test]
    fn set_trims_key() {
        let store = FakeStore::default();
        set_api_key(&store, Some("  sk-abc \n"), Some("unknown")).unwrap();
        assert_eq!(
            Some("sk-abc"),
            store.entries.borrow().get("openai").map(String::as_str)
        );
        assert_eq!(
            Some("sk-abc".to_string()),
            get_api_key(&store, Some("openai"))
        );
    }

    #[test]
    fn get_missing_key_returns_none() {
        let store = FakeStore::default();
        assert_eq!(None, get_api_key(&store, Some("anthropic")));
    }

    #[test]
    fn unavailable_keychain_reads_none_and_reports_set_error() {
        let store = FakeStore {
            broken: true,
            ..FakeStore::default()
        };
        assert_eq!(None, get_api_key(&store, None));
        assert_eq!(
            Err("no keychain".to_string()),
            set_api_key(&store, Some("sk"), None)
        );
    }

    #[test]
    fn decodes_keytar_utf8_bytes_when_utf16_fails() {
        assert_eq!(Some("sk-abc".to_string()), decode_secret(b"sk-abc"));
        assert_eq!(Some("sk-ab".to_string()), decode_secret(b"sk-ab"));
    }

    #[test]
    fn decodes_utf16le_secret() {
        let bytes: Vec<u8> = "sk-abc"
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        assert_eq!(Some("sk-abc".to_string()), decode_secret(&bytes));
    }

    #[test]
    fn undecodable_secret_is_none() {
        assert_eq!(None, decode_secret(&[0xff, 0x00, 0x00]));
    }
}
