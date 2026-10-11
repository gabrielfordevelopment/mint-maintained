use crate::error::GenericError;
use crate::error::ResultExt;

pub const GITHUB_REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");
pub const GITHUB_REQ_USER_AGENT: &str = concat!("mint-maintained/", env!("CARGO_PKG_VERSION"));

fn latest_release_url() -> String {
    let repository = GITHUB_REPOSITORY_URL.trim_start_matches("https://github.com/");
    format!("https://api.github.com/repos/{repository}/releases/latest")
}

pub fn release_download_url(asset_name: &str) -> String {
    format!("{GITHUB_REPOSITORY_URL}/releases/latest/download/{asset_name}")
}

pub fn version_download_url(tag: &str, asset_name: &str) -> String {
    let mut url = reqwest::Url::parse(GITHUB_REPOSITORY_URL).expect("Valid repository URL");
    url.path_segments_mut()
        .unwrap()
        .extend(["releases", "download", tag, asset_name]);
    url.into()
}

#[derive(Debug, serde::Deserialize)]
pub struct GitHubRelease {
    pub html_url: String,
    pub tag_name: String,
    pub body: String,
}

pub async fn get_latest_release() -> Result<GitHubRelease, GenericError> {
    reqwest::Client::builder()
        .user_agent(GITHUB_REQ_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .generic("failed to construct reqwest client".to_string())?
        .get(latest_release_url())
        .send()
        .await
        .generic("check self update request failed".to_string())?
        .error_for_status()
        .generic("update server returned an error".to_string())?
        .json::<GitHubRelease>()
        .await
        .generic("check self update response is error".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn downloads_are_pinned_to_the_displayed_release() {
        assert_eq!(
            super::version_download_url("v0.5.1", "mint-x86_64-pc-windows-msvc.zip"),
            "https://github.com/gabrielfordevelopment/mint-maintained/releases/download/v0.5.1/mint-x86_64-pc-windows-msvc.zip"
        );
        assert!(super::version_download_url("v0.5.1/other", "mint.zip").contains("v0.5.1%2Fother"));
    }

    #[test]
    fn updates_use_the_maintained_repository() {
        assert_eq!(
            super::latest_release_url(),
            "https://api.github.com/repos/gabrielfordevelopment/mint-maintained/releases/latest"
        );
        for asset in [
            "mint-x86_64-pc-windows-msvc.zip",
            "mint-x86_64-unknown-linux-gnu.zip",
        ] {
            assert_eq!(
                super::release_download_url(asset),
                format!(
                    "https://github.com/gabrielfordevelopment/mint-maintained/releases/latest/download/{asset}"
                )
            );
        }
    }
}
