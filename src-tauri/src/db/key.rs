//! The database key lives in the OS keychain (macOS Keychain, Windows
//! Credential Manager, Linux Secret Service), never in a file. It is generated
//! once per install; losing it means the database is unreadable, which is the
//! intended property.

use keyring::Entry;

use super::DbError;

const ACCOUNT: &str = "db-key";
const KEY_BYTES: usize = 32;

/// 32 random bytes as 64 lowercase hex chars (SQLCipher raw-key format).
pub fn generate_hex() -> String {
    let mut buf = [0u8; KEY_BYTES];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn entry(service: &str) -> Result<Entry, DbError> {
    Entry::new(service, ACCOUNT).map_err(|e| DbError::Keychain(e.to_string()))
}

/// Read the key for `service` (use the app identifier), creating and storing a
/// fresh one if none exists yet.
pub fn load_or_create(service: &str) -> Result<String, DbError> {
    let e = entry(service)?;
    match e.get_password() {
        Ok(k) if k.len() == KEY_BYTES * 2 => Ok(k),
        Ok(_) => Err(DbError::Keychain("stored key has the wrong length".into())),
        Err(keyring::Error::NoEntry) => {
            let k = generate_hex();
            e.set_password(&k)
                .map_err(|e| DbError::Keychain(e.to_string()))?;
            Ok(k)
        }
        Err(err) => Err(DbError::Keychain(err.to_string())),
    }
}

/// True when a key is present and readable (health check).
pub fn exists(service: &str) -> bool {
    entry(service)
        .and_then(|e| {
            e.get_password()
                .map_err(|e| DbError::Keychain(e.to_string()))
        })
        .is_ok()
}

/// Remove the key (used by "erase all data"). Missing is not an error.
pub fn delete(service: &str) -> Result<(), DbError> {
    match entry(service)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(DbError::Keychain(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_are_64_hex_chars_and_unique() {
        let a = generate_hex();
        let b = generate_hex();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
