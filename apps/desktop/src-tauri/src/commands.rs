//! Tauri commands exposed to the web UI. Keep these thin: real work lives in
//! the `ot-*` crates.

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub core_version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        core_version: ot_core::VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_matches_workspace_version() {
        let info = app_info();
        assert_eq!(info.version, info.core_version);
    }
}
