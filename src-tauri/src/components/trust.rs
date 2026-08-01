use crate::{
    error::{AppError, AppResult},
    runtime::{
        CapabilityState, CapabilityStatus, SignatureVerifier, COMPONENT_SIGNATURE_DOMAIN,
        S9LAB_COMPONENT_CAPABILITY_ID,
    },
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::signature;
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{collections::BTreeMap, fmt};

const ED25519_PUBLIC_KEY_BYTES: usize = 32;
const ED25519_SIGNATURE_BYTES: usize = 64;
const MAX_TRUSTED_KEYS: usize = 16;

#[derive(Debug, Clone)]
pub struct Ed25519ComponentTrust {
    keys: BTreeMap<String, [u8; ED25519_PUBLIC_KEY_BYTES]>,
    status: CapabilityStatus,
}

impl Ed25519ComponentTrust {
    pub(crate) fn from_compile_time(value: Option<&str>) -> Self {
        match value {
            None | Some("") => Self {
                keys: BTreeMap::new(),
                status: CapabilityStatus::unconfigured(
                    S9LAB_COMPONENT_CAPABILITY_ID,
                    "component_trust_unconfigured",
                ),
            },
            Some(value) => match parse_trust_keys(value) {
                Ok(keys) => Self {
                    keys,
                    status: CapabilityStatus::available(S9LAB_COMPONENT_CAPABILITY_ID),
                },
                Err(_) => Self {
                    keys: BTreeMap::new(),
                    status: CapabilityStatus::disabled(
                        S9LAB_COMPONENT_CAPABILITY_ID,
                        "component_trust_configuration_invalid",
                    ),
                },
            },
        }
    }

    pub fn capability_status(&self) -> CapabilityStatus {
        self.status.clone()
    }
}

impl SignatureVerifier for Ed25519ComponentTrust {
    fn capability_status(&self) -> CapabilityStatus {
        self.capability_status()
    }

    fn verify(
        &self,
        key_id: &str,
        domain: &str,
        payload: &[u8],
        encoded_signature: &str,
    ) -> AppResult<()> {
        if self.status.state != CapabilityState::Available {
            return Err(AppError::coded(self.status.reason_code.clone()));
        }
        if domain != COMPONENT_SIGNATURE_DOMAIN {
            return Err(AppError::coded("component_signature_domain_invalid"));
        }
        let public_key = self
            .keys
            .get(key_id)
            .ok_or_else(|| AppError::coded("component_signature_key_untrusted"))?;
        let signature = decode_signature(encoded_signature)?;
        signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
            .verify(payload, &signature)
            .map_err(|_| AppError::coded("component_signature_invalid"))
    }
}

fn decode_signature(value: &str) -> AppResult<[u8; ED25519_SIGNATURE_BYTES]> {
    if value.contains('=') {
        return Err(AppError::coded("component_signature_encoding_invalid"));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::coded("component_signature_encoding_invalid"))?;
    decoded
        .try_into()
        .map_err(|_| AppError::coded("component_signature_encoding_invalid"))
}

fn parse_trust_keys(value: &str) -> AppResult<BTreeMap<String, [u8; 32]>> {
    let encoded = serde_json::from_str::<UniqueEncodedKeyMap>(value)
        .map_err(|_| AppError::coded("component_trust_configuration_invalid"))?
        .0;
    if encoded.is_empty() || encoded.len() > MAX_TRUSTED_KEYS {
        return Err(AppError::coded("component_trust_configuration_invalid"));
    }

    encoded
        .into_iter()
        .map(|(key_id, encoded_key)| {
            validate_key_id(&key_id)?;
            if encoded_key.contains('=') {
                return Err(AppError::coded("component_trust_configuration_invalid"));
            }
            let decoded = URL_SAFE_NO_PAD
                .decode(encoded_key)
                .map_err(|_| AppError::coded("component_trust_configuration_invalid"))?;
            let key = decoded
                .try_into()
                .map_err(|_| AppError::coded("component_trust_configuration_invalid"))?;
            Ok((key_id, key))
        })
        .collect()
}

fn validate_key_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        || !value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AppError::coded("component_trust_configuration_invalid"));
    }
    Ok(())
}

struct UniqueEncodedKeyMap(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for UniqueEncodedKeyMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyMapVisitor;

        impl<'de> Visitor<'de> for KeyMapVisitor {
            type Value = UniqueEncodedKeyMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object mapping unique key IDs to Ed25519 public keys")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut keys = BTreeMap::new();
                while let Some((key_id, encoded_key)) = map.next_entry::<String, String>()? {
                    if keys.insert(key_id.clone(), encoded_key).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate trust key ID: {key_id}"
                        )));
                    }
                }
                Ok(UniqueEncodedKeyMap(keys))
            }
        }

        deserializer.deserialize_map(KeyMapVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SignatureVerifier;

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected verification failure")
            .descriptor()
            .code
    }

    #[test]
    fn absent_or_malformed_trust_configuration_fails_closed() {
        let absent = Ed25519ComponentTrust::from_compile_time(None);
        assert_eq!(
            absent.capability_status().state,
            CapabilityState::Unconfigured
        );
        assert_eq!(
            error_code(absent.verify(
                "release-1",
                COMPONENT_SIGNATURE_DOMAIN,
                b"payload",
                &URL_SAFE_NO_PAD.encode([0u8; 64])
            )),
            "component_trust_unconfigured"
        );

        for invalid in [
            "{}",
            r#"{"Release-Key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
            r#"{"release-key":"not-base64"}"#,
            r#"{"release-key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","release-key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
        ] {
            assert_eq!(
                Ed25519ComponentTrust::from_compile_time(Some(invalid))
                    .capability_status()
                    .state,
                CapabilityState::Disabled,
                "{invalid}"
            );
        }
    }

    #[test]
    fn rfc8032_public_vector_is_verified_without_a_private_fixture() {
        // RFC 8032, section 7.1, TEST 1: empty message.
        let public_key =
            hex::decode("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
                .expect("public key");
        let signature = hex::decode(concat!(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155",
            "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        ))
        .expect("signature");
        let encoded_keys = format!(
            r#"{{"rfc8032-test-1":"{}"}}"#,
            URL_SAFE_NO_PAD.encode(public_key)
        );
        let trust = Ed25519ComponentTrust::from_compile_time(Some(&encoded_keys));
        assert!(trust.capability_status().is_available());
        trust
            .verify(
                "rfc8032-test-1",
                COMPONENT_SIGNATURE_DOMAIN,
                b"",
                &URL_SAFE_NO_PAD.encode(signature),
            )
            .expect("valid RFC 8032 vector");
    }

    #[test]
    fn unknown_key_domain_and_noncanonical_signature_are_rejected() {
        let public_key =
            hex::decode("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
                .expect("public key");
        let encoded_keys = format!(
            r#"{{"release-1":"{}"}}"#,
            URL_SAFE_NO_PAD.encode(public_key)
        );
        let trust = Ed25519ComponentTrust::from_compile_time(Some(&encoded_keys));
        let signature = URL_SAFE_NO_PAD.encode([0u8; 64]);

        assert_eq!(
            error_code(trust.verify(
                "release-2",
                COMPONENT_SIGNATURE_DOMAIN,
                b"payload",
                &signature
            )),
            "component_signature_key_untrusted"
        );
        assert_eq!(
            error_code(trust.verify("release-1", "OTHER-DOMAIN", b"payload", &signature)),
            "component_signature_domain_invalid"
        );
        assert_eq!(
            error_code(trust.verify(
                "release-1",
                COMPONENT_SIGNATURE_DOMAIN,
                b"payload",
                &format!("{signature}=")
            )),
            "component_signature_encoding_invalid"
        );
    }
}
