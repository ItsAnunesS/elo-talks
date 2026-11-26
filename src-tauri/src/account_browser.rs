//! Shared production browser policy, including persistent account contexts.
use tauri_runtime_cef::{Cef, DebugEnvironment, DevToolsPolicy, RemoteDebugging, SandboxPolicy};

pub fn runtime(profile: &std::path::Path) -> anyhow::Result<Cef> {
    let chromium_root = crate::account_profiles::prepare(profile)?;
    Ok(Cef::default()
        .secret_storage(tauri_runtime_cef::SecretStorage::System)
        .persist_session_cookies(true)
        .sandbox(SandboxPolicy::Required)
        .certificate_errors(tauri_runtime_cef::CertificateErrorPolicy::Cancel)
        .root_cache_path(&chromium_root)
        .with_settings(move |settings| {
            // Bootstrap without opening persistent browser data. Each account
            // receives its own context only after protect_secrets below.
            settings.root_cache_path = chromium_root.to_string_lossy().as_ref().into();
            settings.cache_path = Default::default();
        })
        .debug_environment(DebugEnvironment::Deny)
        .log_severity(tauri_runtime_cef::cef::LogSeverity::DISABLE)
        .remote_debugging(RemoteDebugging::Disabled)
        .devtools(DevToolsPolicy::Disallowed)
        .allow_chromium_command_line_args(false)
        .default_content_setting(
            tauri_runtime_cef::cef::ContentSettingTypes::NOTIFICATIONS,
            tauri_runtime_cef::cef::ContentSettingValues::BLOCK,
        ))
}
