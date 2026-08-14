use ts_rs::{Config, TS};

use crate::{
    ApplicationError, ApplicationErrorCode, ErrorRecoverability, RecoveryAction, RuntimeBootstrap,
    RuntimeHealth, RuntimeSnapshot, RuntimeStateEvent, SessionHealthLevel, SessionId,
    SessionLifecycle, SessionMode, SessionProgress, SessionSource, SessionSourceKind,
    StartSessionRequest, ipc,
};

const GENERATED_HEADER: &str =
    "// Generated from prollyglot-application-runtime. Do not edit by hand.\n";

pub fn typescript_bindings() -> String {
    let config = Config::default();
    let mut output = String::from(GENERATED_HEADER);
    output.push('\n');
    output.push_str(&format!(
        "export const RUNTIME_CONTRACT_VERSION = {} as const;\n",
        crate::APPLICATION_RUNTIME_CONTRACT_VERSION
    ));
    output.push_str(&format!(
        "export const RUNTIME_COMMANDS = {{ bootstrap: {:?} }} as const;\n",
        ipc::BOOTSTRAP_COMMAND
    ));
    output.push_str(&format!(
        "export const RUNTIME_EVENTS = {{ state: {:?} }} as const;\n\n",
        ipc::STATE_EVENT
    ));

    push_declaration::<SessionId>(&config, &mut output);
    push_declaration::<SessionMode>(&config, &mut output);
    push_declaration::<SessionLifecycle>(&config, &mut output);
    push_declaration::<SessionSourceKind>(&config, &mut output);
    push_declaration::<SessionSource>(&config, &mut output);
    push_declaration::<SessionHealthLevel>(&config, &mut output);
    push_declaration::<SessionProgress>(&config, &mut output);
    push_declaration::<RuntimeHealth>(&config, &mut output);
    push_declaration::<ApplicationErrorCode>(&config, &mut output);
    push_declaration::<ErrorRecoverability>(&config, &mut output);
    push_declaration::<RecoveryAction>(&config, &mut output);
    push_declaration::<ApplicationError>(&config, &mut output);
    push_declaration::<StartSessionRequest>(&config, &mut output);
    push_declaration::<RuntimeSnapshot>(&config, &mut output);
    push_declaration::<RuntimeBootstrap>(&config, &mut output);
    push_declaration::<RuntimeStateEvent>(&config, &mut output);
    while output.ends_with("\n\n") {
        output.pop();
    }
    output
}

fn push_declaration<T: TS>(config: &Config, output: &mut String) {
    let mut declaration = T::decl(config);
    let type_offset = if declaration.starts_with("type ") {
        0
    } else {
        declaration
            .find("\ntype ")
            .map(|offset| offset + 1)
            .expect("ts-rs declaration must contain a type declaration")
    };
    declaration.insert_str(type_offset, "export ");
    output.push_str(&declaration);
    output.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_contract_contains_names_types_and_recovery_metadata() {
        let output = typescript_bindings();

        assert!(output.contains("runtime_bootstrap"));
        assert!(output.contains("runtime-state"));
        assert!(output.contains("export type RuntimeSnapshot"));
        assert!(output.contains("export type ApplicationError"));
        assert!(output.contains("suggestedAction: RecoveryAction"));
        assert!(output.contains("revision: number"));
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(typescript_bindings(), typescript_bindings());
    }
}
