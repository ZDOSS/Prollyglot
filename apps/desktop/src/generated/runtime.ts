// Generated from prollyglot-application-runtime. Do not edit by hand.

export const RUNTIME_CONTRACT_VERSION = 4 as const;
export const RUNTIME_COMMANDS = { configurationSnapshot: "configuration_snapshot", updateConfiguration: "update_configuration", bootstrap: "runtime_bootstrap", sourceSnapshot: "source_snapshot", startCapture: "start_capture", stopCapture: "stop_capture", captureStatus: "capture_status", visualCapabilities: "visual_capabilities", visualSourceSnapshot: "visual_source_snapshot", visualStatus: "visual_status", showVisualRegionSelector: "show_visual_region_selector", completeVisualRegionSelection: "complete_visual_region_selection", cancelVisualRegionSelection: "cancel_visual_region_selection", startVisualTranslation: "start_visual_translation", stopVisualTranslation: "stop_visual_translation", updateCaptionPresentation: "update_caption_presentation", updateVisualPresentation: "update_visual_presentation", inferenceResourceStatus: "inference_resource_status", reportInferenceResource: "report_inference_resource" } as const;
export const RUNTIME_EVENTS = { state: "runtime-state", captureStatus: "capture-status", visualStatus: "visual-status", visualText: "visual-text-update", visualClear: "visual-text-clear", visualRegionSelected: "visual-region-selected", visualRegionSelectionCancelled: "visual-region-selection-cancelled", visualRegionSelectorRequest: "visual-region-selector-request", captionPresentation: "caption-presentation", visualPresentation: "visual-presentation", configuration: "configuration-updated" } as const;

export type ViewMode = "full" | "compact";

export type CaptionOutputPreference = "original" | "translated" | "both";

export type BilingualLayout = "stacked" | "sideBySide";

export type OverlayPosition = "topCenter" | "bottomCenter" | "bottomLeft" | "bottomRight";

export type OverlaySettings = { fontFamily: string, fontSize: number, textColor: string, translatedTextColor: string, bilingualLayout: BilingualLayout, backgroundOpacity: number, width: number, maximumLines: number, readingTimeSeconds: number, fadeDurationMs: number, position: OverlayPosition, clickThrough: boolean, };

export type AudioSourcePreference = { "kind": "followSystemDefault" } | { "kind": "playbackDevice", deviceId: string, };

export type CaptionPreferences = { spokenLanguage: string, outputMode: CaptionOutputPreference, translationTarget?: string, audioSource: AudioSourcePreference, };

export type VisualSourcePreference = "applicationWindow" | "display" | "region";

export type VisualDetectionPreference = "focused" | "allText";

export type VisualPreferences = { sourceMode: VisualSourcePreference, sourceLanguage: string, targetLanguage: string, detectionMode: VisualDetectionPreference, windowId?: string, displayId?: string, };

export type ModelPreferences = { speechModelId?: string, visualModelId?: string, };

export type ApplicationConfiguration = { schemaVersion: number, legacyWebviewImported: boolean, viewMode: ViewMode, captions: CaptionPreferences, overlay: OverlaySettings, visual: VisualPreferences, models: ModelPreferences, };

export type ConfigurationSnapshot = { revision: number, config: ApplicationConfiguration, diagnostic?: string, };

export type UpdateConfigurationCommand = { expectedRevision: number, config: ApplicationConfiguration, };

export type SessionId = number;

export type SessionMode = "audioCaptions" | "visualTranslation";

export type SessionLifecycle = "stopped" | "starting" | "running" | "waiting" | "stopping" | "failed";

export type SessionSourceKind = "systemOutput" | "application" | "inputDevice" | "applicationWindow" | "display" | "region";

export type SessionSource = { id: string, kind: SessionSourceKind, label: string, };

export type SessionHealthLevel = "healthy" | "recovering" | "degraded";

export type SessionProgress = "idle" | "preparingModel" | "startingCapture" | "live" | "waitingForSource" | "stopping" | "failed";

export type RuntimeHealth = { level: SessionHealthLevel, progress: SessionProgress, message: string | null, };

export type ApplicationErrorCode = "sessionConflict" | "noActiveSession" | "staleSession" | "invalidTransition" | "startupCancelled" | "workerExited" | "workerPanicked" | "shutdownTimedOut" | "captureUnavailable" | "captureFailed" | "modelUnavailable" | "modelFailed" | "translationFailed" | "configurationInvalid" | "windowOperationFailed" | "internal";

export type ErrorRecoverability = "automatic" | "retryable" | "userActionRequired" | "restartRequired" | "notRecoverable";

export type RecoveryAction = "retry" | "stopAndRetry" | "waitForSource" | "chooseAnotherSource" | "installModel" | "openSettings" | "restartApplication" | "reportIssue";

export type ApplicationError = { code: ApplicationErrorCode, message: string, recoverability: ErrorRecoverability, suggestedAction: RecoveryAction, sessionId: SessionId | null, };

export type StartSessionRequest = { mode: SessionMode, source: SessionSource, };

export type CaptureSelection = { "kind": "systemDefault" } | { "kind": "systemOutput", deviceId: string, } | { "kind": "application", sourceId: string, };

export type CaptureState = "starting" | "capturing" | "waiting" | "stopping" | "stopped" | "failed";

export type PlaybackDevice = { id: string, name: string, isDefault: boolean, };

export type ApplicationSource = { id: string, name: string, instanceCount: number, deviceIds: Array<string>, };

export type SourceSnapshot = { playbackDevices: Array<PlaybackDevice>, applications: Array<ApplicationSource>, };

export type CaptureStatus = { state: CaptureState, peak: number, droppedFrames: number, sourceLabel?: string, message?: string, };

export type StartCaptureCommand = { selection: CaptureSelection, language: string, };

export type VisualSourceKind = "applicationWindow" | "display";

export type VisualSource = { id: string, kind: VisualSourceKind, label: string, x: number, y: number, width: number, height: number, };

export type VisualSourceSnapshot = { windows: Array<VisualSource>, displays: Array<VisualSource>, };

export type PixelRect = { x: number, y: number, width: number, height: number, };

export type VisualRegionSelectorRequest = { displayId: string, width: number, height: number, };

export type VisualRegionSelected = { displayId: string, region: PixelRect, };

export type ShowVisualRegionSelectorCommand = { displayId: string, };

export type CompleteVisualRegionSelectionCommand = { displayId: string, region: PixelRect, };

export type VisualCaptureSelection = { "kind": "applicationWindow", sourceId: string, } | { "kind": "display", sourceId: string, } | { "kind": "region", displayId: string, region: PixelRect, };

export type VisualDetectionMode = "focused" | "allText";

export type VisualCaptureCapabilities = { windowsGraphicsCapture: boolean, systemPicker: boolean, desktopDuplicationExperiment: boolean, message?: string, };

export type VisualState = "starting" | "capturing" | "waiting" | "stopping" | "stopped" | "failed";

export type VisualStatus = { active: boolean, state: VisualState, sourceLabel?: string, framesReceived: number, framesAnalyzed: number, framesUnchanged: number, replacedFrames: number, visibleRegions: number, overlayRegions: number, message?: string, };

export type VisualCaptureGeometry = { label: string, x: number, y: number, width: number, height: number, };

export type VisualRect = { x: number, y: number, width: number, height: number, };

export type StableVisualTextRegion = { trackId: number, textRevision: number, text: string, confidence: number, language?: string, script?: string, bounds: VisualRect, };

export type VisualTextUpdate = { sessionId: SessionId, runtimeRevision: number, source: VisualCaptureGeometry, visible: Array<StableVisualTextRegion>, translationRequests: Array<StableVisualTextRegion>, removedTrackIds: Array<number>, };

export type VisualTextClear = { sessionId: SessionId, runtimeRevision: number, };

export type CaptionOutputMode = "original" | "translated" | "both";

export type CaptionPresentationPhase = "active" | "holding" | "cleared";

export type CaptionPresentationEntry = { key: string, sourceLanguage: string, original: string, translation?: string, translationPending: boolean, isFinal: boolean, };

export type CaptionPresentationFrame = { sessionId: SessionId, runtimeRevision: number, presentationRevision: number, phase: CaptionPresentationPhase, readableAtMs: number, mode: CaptionOutputMode, targetLanguage?: string, entries: Array<CaptionPresentationEntry>, };

export type UpdateCaptionPresentationCommand = { frame: CaptionPresentationFrame, };

export type VisualPresentationRegion = { trackId: number, textRevision: number, original: string, translation?: string, translationPending: boolean, retained: boolean, bounds: VisualRect, };

export type VisualPresentationFrame = { sessionId: SessionId, runtimeRevision: number, presentationRevision: number, sourceWidth: number, sourceHeight: number, sourceLanguage: string, targetLanguage: string, scanning: boolean, regions: Array<VisualPresentationRegion>, };

export type StartVisualTranslationCommand = { selection: VisualCaptureSelection, sourceLanguage: string, targetLanguage: string, detectionMode: VisualDetectionMode | null, };

export type UpdateVisualPresentationCommand = { frame: VisualPresentationFrame, };

export type RuntimeSnapshot = { contractVersion: number, revision: number, sessionId: SessionId | null, mode: SessionMode | null, source: SessionSource | null, lifecycle: SessionLifecycle, health: RuntimeHealth, failure: ApplicationError | null, };

export type RuntimeBootstrap = { snapshot: RuntimeSnapshot, };

export type RuntimeStateEvent = { snapshot: RuntimeSnapshot, };

export type InferenceResourceKind = "speech" | "visualOcr" | "translation";

export type InferenceResourcePhase = "loaded" | "unloaded";

export type ReportInferenceResourceCommand = { sessionId: SessionId, mode: SessionMode, ownerId: string, kind: InferenceResourceKind, phase: InferenceResourcePhase, modelId: string | null, coldStartMillis: number, };

export type InferenceResourceStatus = { sessionId: SessionId, mode: SessionMode, kind: InferenceResourceKind, modelId: string, coldStartMillis: number, residentBytesAtLoad: number | null, };

export type InferenceResourceSnapshot = { revision: number, processResidentBytes: number | null, resources: Array<InferenceResourceStatus>, };


export const DEFAULT_APPLICATION_CONFIGURATION = {"schemaVersion":1,"legacyWebviewImported":false,"viewMode":"full","captions":{"spokenLanguage":"en","outputMode":"original","translationTarget":"en","audioSource":{"kind":"followSystemDefault"}},"overlay":{"fontFamily":"\"Segoe UI Variable\", \"Segoe UI\", sans-serif","fontSize":36,"textColor":"#f4f6f5","translatedTextColor":"#86e3b0","bilingualLayout":"stacked","backgroundOpacity":0.75,"width":720,"maximumLines":3,"readingTimeSeconds":15,"fadeDurationMs":800,"position":"bottomCenter","clickThrough":true},"visual":{"sourceMode":"applicationWindow","sourceLanguage":"ja","targetLanguage":"en","detectionMode":"focused"},"models":{}} as const satisfies ApplicationConfiguration;
