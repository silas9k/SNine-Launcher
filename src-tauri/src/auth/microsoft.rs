use crate::{
    auth::model::AccountSession,
    error::{AppError, AppResult},
};
use chrono::Utc;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::time::sleep;

pub const MICROSOFT_CLIENT_ID: &str = "e686aebd-d575-4472-b163-b0c54f388f43";
const MICROSOFT_SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const MINECRAFT_ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Debug, Clone)]
pub(crate) struct DeviceCodeSecret {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenError {
    error: String,
}

#[derive(Debug, Deserialize)]
struct XboxAuthResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxClaim>,
}

#[derive(Debug, Deserialize)]
struct XboxClaim {
    uhs: String,
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MinecraftLoginResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct MinecraftEntitlementsResponse {
    #[serde(default)]
    items: Vec<MinecraftEntitlement>,
}

#[derive(Debug, Deserialize)]
struct MinecraftEntitlement {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfileResponse {
    id: String,
    name: String,
}

#[derive(Debug)]
pub(crate) struct VerifiedMinecraftSession {
    pub account_id: String,
    pub username: String,
    pub session: AccountSession,
    pub verified_at_unix: i64,
}

#[derive(Clone)]
pub(crate) struct MicrosoftApi {
    client: Client,
}

impl MicrosoftApi {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(50))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }

    pub async fn request_device_code(&self, locale: &str) -> AppResult<DeviceCodeSecret> {
        let locale = match locale {
            "de" => "de-DE",
            "en" => "en-US",
            _ => return Err(AppError::coded("auth_locale_invalid")),
        };
        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .query(&[("mkt", locale)])
            .form(&[
                ("client_id", MICROSOFT_CLIENT_ID),
                ("scope", MICROSOFT_SCOPE),
            ])
            .send()
            .await?;
        let response: DeviceCodeResponse = parse_success_json(response, "device_code").await?;
        validate_verification_uri(&response.verification_uri)?;
        if response.device_code.trim().is_empty() || response.user_code.trim().is_empty() {
            return Err(AppError::coded("auth_device_response_invalid"));
        }
        Ok(DeviceCodeSecret {
            device_code: response.device_code,
            user_code: response.user_code,
            verification_uri: response.verification_uri,
            expires_in: response.expires_in,
            interval: response.interval.max(5),
        })
    }

    pub async fn complete_device_login(
        &self,
        secret: &DeviceCodeSecret,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<VerifiedMinecraftSession> {
        let microsoft = self.poll_token(secret, cancelled).await?;
        let refresh_token = microsoft
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::coded("auth_refresh_token_missing"))?;
        self.exchange_for_minecraft(&microsoft.access_token, refresh_token)
            .await
    }

    pub async fn refresh_session(
        &self,
        refresh_token: &str,
    ) -> AppResult<VerifiedMinecraftSession> {
        if refresh_token.trim().is_empty() {
            return Err(AppError::coded("auth_refresh_token_missing"));
        }
        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", MICROSOFT_CLIENT_ID),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("scope", MICROSOFT_SCOPE),
            ])
            .send()
            .await?;
        let microsoft: MicrosoftTokenResponse = parse_success_json(response, "refresh").await?;
        let rotated_refresh = microsoft
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| refresh_token.to_string());
        self.exchange_for_minecraft(&microsoft.access_token, rotated_refresh)
            .await
    }

    async fn poll_token(
        &self,
        secret: &DeviceCodeSecret,
        cancelled: Arc<AtomicBool>,
    ) -> AppResult<MicrosoftTokenResponse> {
        let deadline = Instant::now() + Duration::from_secs(secret.expires_in.max(60));
        let mut interval = secret.interval.max(5);
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err(AppError::coded("auth_device_login_cancelled"));
            }
            sleep(Duration::from_secs(interval)).await;
            if cancelled.load(Ordering::Acquire) {
                return Err(AppError::coded("auth_device_login_cancelled"));
            }
            let response = self
                .client
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", MICROSOFT_CLIENT_ID),
                    ("device_code", secret.device_code.as_str()),
                ])
                .send()
                .await?;
            if response.status().is_success() {
                return response
                    .json()
                    .await
                    .map_err(|_| AppError::coded("auth_token_response_invalid"));
            }
            let status = response.status();
            let error = response
                .json::<MicrosoftTokenError>()
                .await
                .map_err(|_| remote_error("device_token", status))?;
            match error.error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval = interval.saturating_add(5).min(30);
                    continue;
                }
                "authorization_declined" => {
                    return Err(AppError::coded("auth_authorization_declined"));
                }
                "expired_token" => return Err(AppError::coded("auth_device_code_expired")),
                "bad_verification_code" => {
                    return Err(AppError::coded("auth_device_code_invalid"));
                }
                _ => return Err(remote_error("device_token", status)),
            }
        }
        Err(AppError::coded("auth_device_code_expired"))
    }

    async fn exchange_for_minecraft(
        &self,
        microsoft_access_token: &str,
        refresh_token: String,
    ) -> AppResult<VerifiedMinecraftSession> {
        let xbl = self.authenticate_xbox_live(microsoft_access_token).await?;
        let xbl_claim = xbl
            .display_claims
            .xui
            .first()
            .ok_or_else(|| AppError::coded("auth_xbox_claim_missing"))?;
        let user_hash = xbl_claim.uhs.clone();
        let xsts = self.authorize_xsts(&xbl.token).await?;
        let xuid = xsts
            .display_claims
            .xui
            .first()
            .and_then(|claim| claim.xid.clone())
            .or_else(|| xbl_claim.xid.clone());
        let minecraft = self.login_minecraft(&user_hash, &xsts.token).await?;
        self.verify_entitlement(&minecraft.access_token).await?;
        let profile = self
            .fetch_minecraft_profile(&minecraft.access_token)
            .await?;
        let now = Utc::now().timestamp();
        Ok(VerifiedMinecraftSession {
            account_id: profile.id,
            username: profile.name,
            session: AccountSession {
                microsoft_refresh_token: refresh_token,
                minecraft_access_token: Some(minecraft.access_token),
                minecraft_expires_at_unix: now.saturating_add(minecraft.expires_in.max(0)),
                xuid,
            },
            verified_at_unix: now,
        })
    }

    async fn authenticate_xbox_live(
        &self,
        microsoft_access_token: &str,
    ) -> AppResult<XboxAuthResponse> {
        self.send_xbox_request(
            XBOX_AUTH_URL,
            json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={microsoft_access_token}")
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }),
        )
        .await
    }

    async fn authorize_xsts(&self, xbox_token: &str) -> AppResult<XboxAuthResponse> {
        self.send_xbox_request(
            XSTS_URL,
            json!({
                "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbox_token] },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }),
        )
        .await
    }

    async fn send_xbox_request(&self, url: &str, body: Value) -> AppResult<XboxAuthResponse> {
        let response = self.client.post(url).json(&body).send().await?;
        let status = response.status();
        if response.status().is_success() {
            return response
                .json()
                .await
                .map_err(|_| AppError::coded("auth_xbox_response_invalid"));
        }
        let xerr = response
            .json::<Value>()
            .await
            .ok()
            .and_then(|value| value.get("XErr").and_then(Value::as_i64));
        let code = match xerr {
            Some(2148916233) => "auth_xbox_profile_missing",
            Some(2148916235) => "auth_xbox_region_blocked",
            Some(2148916236 | 2148916237) => "auth_xbox_age_verification_required",
            Some(2148916238) => "auth_xbox_family_approval_required",
            _ => "auth_xbox_failed",
        };
        Err(AppError::coded_with(
            code,
            [("status", status.as_u16().to_string())],
        ))
    }

    async fn login_minecraft(
        &self,
        user_hash: &str,
        xsts_token: &str,
    ) -> AppResult<MinecraftLoginResponse> {
        let response = self
            .client
            .post(MINECRAFT_LOGIN_URL)
            .json(&json!({ "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}") }))
            .send()
            .await?;
        parse_success_json(response, "minecraft_login").await
    }

    async fn verify_entitlement(&self, access_token: &str) -> AppResult<()> {
        let response = self
            .client
            .get(MINECRAFT_ENTITLEMENTS_URL)
            .bearer_auth(access_token)
            .send()
            .await?;
        let entitlements: MinecraftEntitlementsResponse =
            parse_success_json(response, "minecraft_entitlements").await?;
        if owns_minecraft_java(&entitlements) {
            Ok(())
        } else {
            Err(AppError::coded("auth_minecraft_ownership_missing"))
        }
    }

    async fn fetch_minecraft_profile(
        &self,
        access_token: &str,
    ) -> AppResult<MinecraftProfileResponse> {
        let response = self
            .client
            .get(MINECRAFT_PROFILE_URL)
            .bearer_auth(access_token)
            .send()
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::coded("auth_minecraft_profile_missing"));
        }
        parse_success_json(response, "minecraft_profile").await
    }
}

fn owns_minecraft_java(entitlements: &MinecraftEntitlementsResponse) -> bool {
    entitlements.items.iter().any(|item| {
        let name = item.name.to_ascii_lowercase();
        name == "game_minecraft" || name == "product_minecraft"
    })
}

fn validate_verification_uri(value: &str) -> AppResult<()> {
    let url =
        reqwest::Url::parse(value).map_err(|_| AppError::coded("auth_verification_uri_invalid"))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::coded("auth_verification_uri_invalid"))?;
    let allowed = host.eq_ignore_ascii_case("microsoft.com")
        || host.ends_with(".microsoft.com")
        || host.eq_ignore_ascii_case("microsoftonline.com")
        || host.ends_with(".microsoftonline.com");
    if url.scheme() != "https"
        || !allowed
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(AppError::coded("auth_verification_uri_invalid"));
    }
    Ok(())
}

async fn parse_success_json<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    service: &str,
) -> AppResult<T> {
    let status = response.status();
    if !status.is_success() {
        return Err(remote_error(service, status));
    }
    response
        .json()
        .await
        .map_err(|_| AppError::coded_with("auth_remote_response_invalid", [("service", service)]))
}

fn remote_error(service: &str, status: StatusCode) -> AppError {
    AppError::coded_with(
        "auth_remote_error",
        [
            ("service", service.to_string()),
            ("status", status.as_u16().to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_requires_a_minecraft_entitlement() {
        let owned: MinecraftEntitlementsResponse =
            serde_json::from_str(r#"{"items":[{"name":"game_minecraft"}]}"#)
                .expect("owned fixture");
        let empty: MinecraftEntitlementsResponse =
            serde_json::from_str(r#"{"items":[]}"#).expect("empty fixture");
        let lookalike: MinecraftEntitlementsResponse =
            serde_json::from_str(r#"{"items":[{"name":"unrelated_minecraft_preview"}]}"#)
                .expect("lookalike fixture");
        assert!(owns_minecraft_java(&owned));
        assert!(!owns_minecraft_java(&empty));
        assert!(!owns_minecraft_java(&lookalike));
    }

    #[test]
    fn only_expected_https_verification_hosts_are_accepted() {
        assert!(validate_verification_uri("https://www.microsoft.com/link").is_ok());
        let non_https = ["http", "://www.microsoft.com/link"].concat();
        let reserved_host = ["https://microsoft.com", ".invalid/link"].concat();
        let embedded_credentials = ["https", "://user:pass@www.microsoft.com/link"].concat();
        assert!(validate_verification_uri(&non_https).is_err());
        assert!(validate_verification_uri(&reserved_host).is_err());
        assert!(validate_verification_uri(&embedded_credentials).is_err());
    }

    #[test]
    fn remote_errors_never_contain_response_bodies() {
        let error = remote_error("fixture", StatusCode::UNAUTHORIZED).descriptor();
        assert_eq!(error.code, "auth_remote_error");
        assert_eq!(error.params.get("status").map(String::as_str), Some("401"));
        assert_eq!(error.params.len(), 2);
    }
}
