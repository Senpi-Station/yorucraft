use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub access_token: String,
    pub uuid: String,
    pub username: String,
    pub user_type: String,
    pub skin_url: Option<String>,
    pub skin_model: Option<String>,
}

pub struct OfflineAuth {
    username: String,
    uuid_cache: Option<String>,
}

impl OfflineAuth {
    pub fn new(username: &str) -> Self {
        Self {
            username: username.to_string(),
            uuid_cache: None,
        }
    }

    pub fn uuid(&mut self) -> String {
        if let Some(ref cached) = self.uuid_cache {
            return cached.clone();
        }
        let uuid = offline_uuid(&self.username);
        self.uuid_cache = Some(uuid.clone());
        uuid
    }

    pub fn access_token(&self) -> String {
        "0".to_string()
    }

    pub fn user_type(&self) -> String {
        "legacy".to_string()
    }

    pub fn to_auth_result(&mut self) -> AuthResult {
        AuthResult {
            access_token: self.access_token(),
            uuid: self.uuid(),
            username: self.username.clone(),
            user_type: self.user_type(),
            skin_url: None,
            skin_model: None,
        }
    }
}

pub fn offline_uuid(username: &str) -> String {
    let input = format!("OfflinePlayer:{}", username);
    let digest = Md5::digest(input.as_bytes());
    let mut bytes: [u8; 16] = digest.into();

    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_uuid_notch() {
        assert_eq!(
            offline_uuid("Notch"),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
    }

    #[test]
    fn test_offline_uuid_case_sensitive() {
        assert_ne!(offline_uuid("notch"), offline_uuid("Notch"));
    }

    #[test]
    fn test_offline_uuid_format() {
        let uuid = offline_uuid("test");
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.matches('-').count(), 4);
    }

    #[test]
    fn test_offline_auth() {
        let mut auth = OfflineAuth::new("Steve");
        assert_eq!(auth.access_token(), "0");
        assert_eq!(auth.user_type(), "legacy");

        let uuid1 = auth.uuid();
        let uuid2 = auth.uuid();
        assert_eq!(uuid1, uuid2);

        let result = auth.to_auth_result();
        assert_eq!(result.username, "Steve");
        assert_eq!(result.access_token, "0");
        assert_eq!(result.user_type, "legacy");
        assert!(result.skin_url.is_none());
    }
}
