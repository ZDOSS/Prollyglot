use prollyglot_core::SourceId;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdentityKind {
    ApplicationUserModel,
    PackageFamily,
    ExecutablePath,
    ExecutableName,
}

impl IdentityKind {
    const fn tag(self) -> &'static str {
        match self {
            Self::ApplicationUserModel => "aumid",
            Self::PackageFamily => "package",
            Self::ExecutablePath => "path",
            Self::ExecutableName => "executable",
        }
    }
}

/// Produces a stable, opaque identifier without publishing the executable path
/// or Windows package identity used to derive it.
pub(crate) fn stable_application_id(kind: IdentityKind, value: &str) -> SourceId {
    let normalized = value.trim().replace('\\', "/").to_lowercase();
    let mut digest = Sha256::new();
    digest.update(b"prollyglot/windows-application/v1\0");
    digest.update(kind.tag().as_bytes());
    digest.update(b"\0");
    digest.update(normalized.as_bytes());
    let digest = digest.finalize();
    let mut encoded = String::with_capacity(4 + 32);
    encoded.push_str("app:");
    for byte in &digest[..16] {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    SourceId::new(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_opaque_stable_and_case_insensitive_for_windows_material() {
        let first = stable_application_id(
            IdentityKind::ExecutablePath,
            r"C:\Program Files\Player\PLAYER.EXE",
        );
        let restarted = stable_application_id(
            IdentityKind::ExecutablePath,
            "c:/program files/player/player.exe",
        );
        assert_eq!(first, restarted);
        assert!(first.0.starts_with("app:"));
        assert!(!first.0.contains("player"));
    }

    #[test]
    fn identity_namespaces_prevent_cross_kind_aliasing() {
        let package = stable_application_id(IdentityKind::PackageFamily, "example.player");
        let executable = stable_application_id(IdentityKind::ExecutableName, "example.player");
        let aumid = stable_application_id(IdentityKind::ApplicationUserModel, "example.player");
        assert_ne!(package, executable);
        assert_ne!(package, aumid);
    }
}
