import type { ApplicationError } from "./types";

export function isApplicationError(error: unknown): error is ApplicationError {
  if (!error || typeof error !== "object") return false;
  const candidate = error as Partial<ApplicationError>;
  return typeof candidate.code === "string"
    && typeof candidate.message === "string"
    && typeof candidate.recoverability === "string"
    && typeof candidate.suggestedAction === "string";
}

export function errorMessage(
  error: unknown,
  fallback = "Prollyglot could not complete that action."
): string {
  if (error instanceof Error) return error.message;
  if (isApplicationError(error)) return error.message;
  if (typeof error === "string") return error;
  return fallback;
}
