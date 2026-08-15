use ts_rs::{Config, TS};

use crate::{
    ApplicationConfiguration, ApplicationError, ApplicationErrorCode, ApplicationSource,
    AudioSourcePreference, BilingualLayout, CaptionOutputMode, CaptionOutputPreference,
    CaptionPreferences, CaptionPresentationEntry, CaptionPresentationFrame,
    CaptionPresentationPhase, CaptureSelection, CaptureState, CaptureStatus,
    CompleteVisualRegionSelectionCommand, ConfigurationSnapshot, ErrorRecoverability,
    InferenceResourceKind, InferenceResourcePhase, InferenceResourceSnapshot,
    InferenceResourceStatus, ModelPreferences, OverlayPosition, OverlaySettings, PixelRect,
    PlaybackDevice, RecoveryAction, ReportInferenceResourceCommand, RuntimeBootstrap,
    RuntimeHealth, RuntimeSnapshot, RuntimeStateEvent, SessionHealthLevel, SessionId,
    SessionLifecycle, SessionMode, SessionProgress, SessionSource, SessionSourceKind,
    ShowVisualRegionSelectorCommand, SourceSnapshot, StableVisualTextRegion, StartCaptureCommand,
    StartSessionRequest, StartVisualTranslationCommand, UpdateCaptionPresentationCommand,
    UpdateConfigurationCommand, UpdateVisualPresentationCommand, ViewMode,
    VisualCaptureCapabilities, VisualCaptureGeometry, VisualCaptureSelection, VisualDetectionMode,
    VisualDetectionPreference, VisualPreferences, VisualPresentationFrame,
    VisualPresentationRegion, VisualRect, VisualRegionSelected, VisualRegionSelectorRequest,
    VisualSource, VisualSourceKind, VisualSourcePreference, VisualSourceSnapshot, VisualState,
    VisualStatus, VisualTextClear, VisualTextUpdate, ipc,
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
        concat!(
            "export const RUNTIME_COMMANDS = {{ ",
            "configurationSnapshot: {:?}, updateConfiguration: {:?}, ",
            "bootstrap: {:?}, sourceSnapshot: {:?}, startCapture: {:?}, stopCapture: {:?}, ",
            "captureStatus: {:?}, visualCapabilities: {:?}, visualSourceSnapshot: {:?}, ",
            "visualStatus: {:?}, showVisualRegionSelector: {:?}, ",
            "completeVisualRegionSelection: {:?}, cancelVisualRegionSelection: {:?}, ",
            "startVisualTranslation: {:?}, stopVisualTranslation: {:?}, ",
            "updateCaptionPresentation: {:?}, updateVisualPresentation: {:?}, ",
            "inferenceResourceStatus: {:?}, reportInferenceResource: {:?} ",
            "}} as const;\n"
        ),
        ipc::CONFIGURATION_SNAPSHOT_COMMAND,
        ipc::UPDATE_CONFIGURATION_COMMAND,
        ipc::BOOTSTRAP_COMMAND,
        ipc::SOURCE_SNAPSHOT_COMMAND,
        ipc::START_CAPTURE_COMMAND,
        ipc::STOP_CAPTURE_COMMAND,
        ipc::CAPTURE_STATUS_COMMAND,
        ipc::VISUAL_CAPABILITIES_COMMAND,
        ipc::VISUAL_SOURCE_SNAPSHOT_COMMAND,
        ipc::VISUAL_STATUS_COMMAND,
        ipc::SHOW_VISUAL_REGION_SELECTOR_COMMAND,
        ipc::COMPLETE_VISUAL_REGION_SELECTION_COMMAND,
        ipc::CANCEL_VISUAL_REGION_SELECTION_COMMAND,
        ipc::START_VISUAL_TRANSLATION_COMMAND,
        ipc::STOP_VISUAL_TRANSLATION_COMMAND,
        ipc::UPDATE_CAPTION_PRESENTATION_COMMAND,
        ipc::UPDATE_VISUAL_PRESENTATION_COMMAND,
        ipc::INFERENCE_RESOURCE_STATUS_COMMAND,
        ipc::REPORT_INFERENCE_RESOURCE_COMMAND,
    ));
    output.push_str(&format!(
        concat!(
            "export const RUNTIME_EVENTS = {{ state: {:?}, captureStatus: {:?}, ",
            "visualStatus: {:?}, visualText: {:?}, visualClear: {:?}, ",
            "visualRegionSelected: {:?}, visualRegionSelectionCancelled: {:?}, ",
            "visualRegionSelectorRequest: {:?}, captionPresentation: {:?}, ",
            "visualPresentation: {:?}, configuration: {:?} }} as const;\n\n"
        ),
        ipc::STATE_EVENT,
        ipc::CAPTURE_STATUS_EVENT,
        ipc::VISUAL_STATUS_EVENT,
        ipc::VISUAL_TEXT_EVENT,
        ipc::VISUAL_CLEAR_EVENT,
        ipc::VISUAL_REGION_SELECTED_EVENT,
        ipc::VISUAL_REGION_SELECTION_CANCELLED_EVENT,
        ipc::VISUAL_REGION_SELECTOR_REQUEST_EVENT,
        ipc::CAPTION_PRESENTATION_EVENT,
        ipc::VISUAL_PRESENTATION_EVENT,
        ipc::CONFIGURATION_EVENT,
    ));

    push_declaration::<ViewMode>(&config, &mut output);
    push_declaration::<CaptionOutputPreference>(&config, &mut output);
    push_declaration::<BilingualLayout>(&config, &mut output);
    push_declaration::<OverlayPosition>(&config, &mut output);
    push_declaration::<OverlaySettings>(&config, &mut output);
    push_declaration::<AudioSourcePreference>(&config, &mut output);
    push_declaration::<CaptionPreferences>(&config, &mut output);
    push_declaration::<VisualSourcePreference>(&config, &mut output);
    push_declaration::<VisualDetectionPreference>(&config, &mut output);
    push_declaration::<VisualPreferences>(&config, &mut output);
    push_declaration::<ModelPreferences>(&config, &mut output);
    push_declaration::<ApplicationConfiguration>(&config, &mut output);
    push_declaration::<ConfigurationSnapshot>(&config, &mut output);
    push_declaration::<UpdateConfigurationCommand>(&config, &mut output);
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
    push_declaration::<CaptureSelection>(&config, &mut output);
    push_declaration::<CaptureState>(&config, &mut output);
    push_declaration::<PlaybackDevice>(&config, &mut output);
    push_declaration::<ApplicationSource>(&config, &mut output);
    push_declaration::<SourceSnapshot>(&config, &mut output);
    push_declaration::<CaptureStatus>(&config, &mut output);
    push_declaration::<StartCaptureCommand>(&config, &mut output);
    push_declaration::<VisualSourceKind>(&config, &mut output);
    push_declaration::<VisualSource>(&config, &mut output);
    push_declaration::<VisualSourceSnapshot>(&config, &mut output);
    push_declaration::<PixelRect>(&config, &mut output);
    push_declaration::<VisualRegionSelectorRequest>(&config, &mut output);
    push_declaration::<VisualRegionSelected>(&config, &mut output);
    push_declaration::<ShowVisualRegionSelectorCommand>(&config, &mut output);
    push_declaration::<CompleteVisualRegionSelectionCommand>(&config, &mut output);
    push_declaration::<VisualCaptureSelection>(&config, &mut output);
    push_declaration::<VisualDetectionMode>(&config, &mut output);
    push_declaration::<VisualCaptureCapabilities>(&config, &mut output);
    push_declaration::<VisualState>(&config, &mut output);
    push_declaration::<VisualStatus>(&config, &mut output);
    push_declaration::<VisualCaptureGeometry>(&config, &mut output);
    push_declaration::<VisualRect>(&config, &mut output);
    push_declaration::<StableVisualTextRegion>(&config, &mut output);
    push_declaration::<VisualTextUpdate>(&config, &mut output);
    push_declaration::<VisualTextClear>(&config, &mut output);
    push_declaration::<CaptionOutputMode>(&config, &mut output);
    push_declaration::<CaptionPresentationPhase>(&config, &mut output);
    push_declaration::<CaptionPresentationEntry>(&config, &mut output);
    push_declaration::<CaptionPresentationFrame>(&config, &mut output);
    push_declaration::<UpdateCaptionPresentationCommand>(&config, &mut output);
    push_declaration::<VisualPresentationRegion>(&config, &mut output);
    push_declaration::<VisualPresentationFrame>(&config, &mut output);
    push_declaration::<StartVisualTranslationCommand>(&config, &mut output);
    push_declaration::<UpdateVisualPresentationCommand>(&config, &mut output);
    push_declaration::<RuntimeSnapshot>(&config, &mut output);
    push_declaration::<RuntimeBootstrap>(&config, &mut output);
    push_declaration::<RuntimeStateEvent>(&config, &mut output);
    push_declaration::<InferenceResourceKind>(&config, &mut output);
    push_declaration::<InferenceResourcePhase>(&config, &mut output);
    push_declaration::<ReportInferenceResourceCommand>(&config, &mut output);
    push_declaration::<InferenceResourceStatus>(&config, &mut output);
    push_declaration::<InferenceResourceSnapshot>(&config, &mut output);
    while output.ends_with("\n\n") {
        output.pop();
    }
    let default_configuration = serde_json::to_string(&ApplicationConfiguration::default())
        .expect("default application configuration must serialize");
    output.push_str("\n\nexport const DEFAULT_APPLICATION_CONFIGURATION = ");
    output.push_str(&default_configuration);
    output.push_str(" as const satisfies ApplicationConfiguration;\n");
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
        assert!(output.contains("configuration_snapshot"));
        assert!(output.contains("configuration-updated"));
        assert!(output.contains("runtime-state"));
        assert!(output.contains("start_capture"));
        assert!(output.contains("start_visual_translation"));
        assert!(output.contains("update_caption_presentation"));
        assert!(output.contains("caption-presentation"));
        assert!(output.contains("visual-text-update"));
        assert!(output.contains("export type RuntimeSnapshot"));
        assert!(output.contains("export type ApplicationConfiguration"));
        assert!(output.contains("export type ConfigurationSnapshot"));
        assert!(output.contains("export const DEFAULT_APPLICATION_CONFIGURATION"));
        assert!(output.contains("export type ApplicationError"));
        assert!(output.contains("export type CaptureSelection"));
        assert!(output.contains("export type VisualCaptureSelection"));
        assert!(output.contains("export type VisualTextUpdate"));
        assert!(output.contains("export type CaptionPresentationFrame"));
        assert!(output.contains("export type VisualPresentationFrame"));
        assert!(output.contains("suggestedAction: RecoveryAction"));
        assert!(output.contains("revision: number"));
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(typescript_bindings(), typescript_bindings());
    }
}
