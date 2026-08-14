// Generated from prollyglot-application-runtime. Do not edit by hand.

export const RUNTIME_CONTRACT_VERSION = 1 as const;
export const RUNTIME_COMMANDS = { bootstrap: "runtime_bootstrap" } as const;
export const RUNTIME_EVENTS = { state: "runtime-state" } as const;

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

export type RuntimeSnapshot = { contractVersion: number, revision: number, sessionId: SessionId | null, mode: SessionMode | null, source: SessionSource | null, lifecycle: SessionLifecycle, health: RuntimeHealth, failure: ApplicationError | null, };

export type RuntimeBootstrap = { snapshot: RuntimeSnapshot, };

export type RuntimeStateEvent = { snapshot: RuntimeSnapshot, };
