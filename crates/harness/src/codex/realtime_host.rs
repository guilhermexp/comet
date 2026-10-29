//! Codex package resolver, shared with V2 desktop media: both use the audio
//! helper of the device's standalone Codex installation.
pub(super) use zeron_voice_media::NativeHost;

pub(super) fn helper_path(
    executable: &std::path::Path,
) -> Result<std::path::PathBuf, zeron_proto::voice::VoiceRejection> {
    zeron_voice_media::media_helper(Some(executable))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn local_voice_resolves_active_package_with_npm_chat_cli() {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        const CHILD: &str = "ZERON_TEST_VOICE_RESOLUTION_ROOT";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = Path::new(&root);
            let npm = root.join("npm/bin/codex");
            let expected = root.join(
                ".codex/packages/app-server-daemon/current/codex-resources/voice/bin/codex-voice-host",
            );
            // Exercise both consumers with isolated HOME/CODEX_HOME/PATH.
            let resolved = helper_path(&npm).unwrap();
            assert_eq!(resolved, expected);
            assert!(zeron_voice_media::DesktopMedia::for_codex(Some(&npm)).is_ok());
            assert_eq!(std::fs::read_to_string(npm).unwrap(), "npm fixture");
            std::fs::write(root.join("resolved-helper"), resolved.to_str().unwrap()).unwrap();
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let package = root.join(".codex/packages/app-server-daemon/current");
        let npm = root.join("npm/bin/codex");
        let helper = package.join("codex-resources/voice/bin/codex-voice-host");
        std::fs::create_dir_all(npm.parent().unwrap()).unwrap();
        std::fs::write(&npm, "npm fixture").unwrap();
        std::fs::create_dir_all(package.join("bin")).unwrap();
        std::fs::write(package.join("bin/codex"), "unused CLI fixture").unwrap();
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(&helper, "unused media fixture").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let target = format!(
            "{}-{}",
            std::env::consts::ARCH,
            if cfg!(target_os = "macos") {
                "apple-darwin"
            } else {
                "unknown-linux-gnu"
            }
        );
        std::fs::write(
            package.join("codex-package.json"),
            serde_json::json!({
                "layoutVersion":1,"version":"0.165.0","variant":"codex",
                "target":target,"entrypoint":"bin/codex","resourcesDir":"codex-resources",
                "pathDir":"codex-path"
            })
            .to_string(),
        )
        .unwrap();
        for explicit_home in [false, true] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "codex::realtime::host::tests::local_voice_resolves_active_package_with_npm_chat_cli",
                    "--nocapture",
                ])
                .env_remove("ZERON_VOICE_MEDIA_DIR")
                .env_remove("CODEX_HOME")
                .env(CHILD, &root)
                .env("HOME", &root)
                .env("PATH", npm.parent().unwrap());
            if explicit_home {
                command
                    .env("CODEX_HOME", root.join(".codex"))
                    .env("HOME", root.join("unused-home"));
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            // Also prove the child test ran (a stale --exact filter runs zero tests).
            let receipt = root.join("resolved-helper");
            assert_eq!(
                std::fs::read_to_string(&receipt).unwrap(),
                helper.to_str().unwrap()
            );
            std::fs::remove_file(receipt).unwrap();
        }
    }
}
