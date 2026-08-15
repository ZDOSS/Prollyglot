#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

import { auditSoakLog, formatSoakAudit } from "./soak-log.mjs";

try {
  const { logPath, minSessions, maxProfileGrowthBytes } = parseArguments(process.argv.slice(2));
  const resolvedLog = logPath ? resolve(logPath) : latestWindowsLog();
  if (!resolvedLog || !existsSync(resolvedLog)) {
    console.error(
      "No Prollyglot log was found. Pass its path explicitly: node scripts/check-soak-log.mjs C:\\path\\to\\prollyglot.log"
    );
    process.exitCode = 2;
  } else {
    const audit = auditSoakLog(readFileSync(resolvedLog, "utf8"), {
      minSessions,
      maxProfileGrowthBytes
    });
    console.log(`Log: ${resolvedLog}`);
    console.log(formatSoakAudit(audit));
    if (!audit.ok) process.exitCode = 1;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 2;
}

function parseArguments(arguments_) {
  let logPath;
  let minSessions = 10;
  let maxProfileGrowthBytes = 384 * 1_048_576;
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--min-sessions") {
      minSessions = requiredNumber(arguments_, ++index, argument);
    } else if (argument === "--max-profile-growth-mib") {
      maxProfileGrowthBytes = requiredNumber(arguments_, ++index, argument) * 1_048_576;
    } else if (argument?.startsWith("--")) {
      throw new Error(`Unknown option ${argument}.`);
    } else if (argument) {
      if (logPath) throw new Error("Pass at most one log path.");
      logPath = argument;
    }
  }
  return { logPath, minSessions, maxProfileGrowthBytes };
}

function requiredNumber(arguments_, index, option) {
  const value = Number(arguments_[index]);
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${option} requires a non-negative integer.`);
  }
  return value;
}

function latestWindowsLog() {
  const localAppData = process.env.LOCALAPPDATA;
  if (!localAppData) return undefined;
  const directory = join(localAppData, "com.prollyglot.desktop", "logs");
  if (!existsSync(directory)) return undefined;
  return readdirSync(directory)
    .filter((name) => name.endsWith(".log"))
    .map((name) => join(directory, name))
    .sort((left, right) => statSync(right).mtimeMs - statSync(left).mtimeMs)[0];
}
