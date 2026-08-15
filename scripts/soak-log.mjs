const MIB = 1_048_576;
const RESOURCE_KINDS = ["Speech", "VisualOcr", "Translation"];

export function auditSoakLog(logText, options = {}) {
  const minSessions = integerOption(options.minSessions, 10);
  const maxProfileGrowthBytes = integerOption(
    options.maxProfileGrowthBytes,
    384 * MIB
  );
  const requiredKinds = options.requiredKinds ?? RESOURCE_KINDS;
  const lines = logText.split(/\r?\n/u).filter(Boolean);
  const sessions = new Map();
  const activeResources = new Map();
  const loadsByKind = new Map(RESOURCE_KINDS.map((kind) => [kind, 0]));
  const releaseSamples = [];
  const privacyViolations = [];
  let stoppedSnapshots = 0;
  let failedSnapshots = 0;
  let peakResidentBytes = 0;
  let launchEpoch = 0;

  for (const line of lines) {
    if (line.includes("Prollyglot started")) {
      launchEpoch += 1;
    }

    if (line.includes("runtime state changed")) {
      const sessionId = numberField(line, "session_id");
      const lifecycle = wordField(line, "lifecycle");
      if (lifecycle === "Starting" && sessionId > 0) {
        const key = sessionKey(launchEpoch, sessionId);
        const session = sessions.get(key) ?? { profile: new Set(), started: false };
        session.started = true;
        sessions.set(key, session);
      }
      if (lifecycle === "Stopped") stoppedSnapshots += 1;
      if (lifecycle === "Failed") failedSnapshots += 1;
    }

    if (line.includes("local inference resource loaded")) {
      const resource = resourceFields(line);
      if (resource) {
        const key = resourceKey(launchEpoch, resource);
        activeResources.set(key, (activeResources.get(key) ?? 0) + 1);
        loadsByKind.set(resource.kind, (loadsByKind.get(resource.kind) ?? 0) + 1);
        const keyForSession = sessionKey(launchEpoch, resource.sessionId);
        const session = sessions.get(keyForSession) ?? { profile: new Set(), started: false };
        session.profile.add(`${resource.kind}:${resource.modelId}`);
        sessions.set(keyForSession, session);
      }
      peakResidentBytes = Math.max(peakResidentBytes, residentBytes(line));
    }

    if (line.includes("local inference resource unloaded")
      || line.includes("local inference resource force-released")) {
      const resource = resourceFields(line);
      if (resource) {
        const key = resourceKey(launchEpoch, resource);
        const remaining = (activeResources.get(key) ?? 0) - 1;
        if (remaining > 0) activeResources.set(key, remaining);
        else activeResources.delete(key);
      }
    }

    if (line.includes("session inference resources released")) {
      const sessionId = numberField(line, "session_id");
      const bytes = residentBytes(line);
      const profile = [
        ...(sessions.get(sessionKey(launchEpoch, sessionId))?.profile ?? [])
      ].sort().join("+") || "none";
      if (bytes > 0) releaseSamples.push({ launchEpoch, sessionId, profile, bytes });
    }

    if (/(?:caption_text|recognized_text|translated_text|audio_samples|frame_pixels)=/iu.test(line)) {
      privacyViolations.push(line);
    }
  }

  const startedSessions = [...sessions.values()].filter(({ started }) => started).length;
  const profileMemory = summarizeProfileMemory(releaseSamples);
  const failures = [];
  if (startedSessions < minSessions) {
    failures.push(`Only ${startedSessions} sessions started; the soak requires at least ${minSessions}.`);
  }
  for (const kind of requiredKinds) {
    if ((loadsByKind.get(kind) ?? 0) === 0) {
      failures.push(`No ${kind} inference load was observed.`);
    }
  }
  if (activeResources.size > 0) {
    failures.push(`${activeResources.size} inference resource ownership record(s) remained loaded.`);
  }
  for (const profile of profileMemory) {
    if (profile.samples >= 3 && profile.growthBytes > maxProfileGrowthBytes) {
      failures.push(
        `${profile.profile} grew ${formatMib(profile.growthBytes)} across comparable post-stop samples.`
      );
    }
  }
  if (privacyViolations.length > 0) {
    failures.push(`${privacyViolations.length} forbidden media-content log field(s) were found.`);
  }
  if (releaseSamples.length === 0) {
    failures.push("No post-session resident-memory samples were found.");
  }

  return {
    ok: failures.length === 0,
    failures,
    startedSessions,
    stoppedSnapshots,
    failedSnapshots,
    loadsByKind: Object.fromEntries(loadsByKind),
    activeResources: [...activeResources.keys()],
    peakResidentBytes,
    releaseSamples,
    profileMemory,
    privacyViolations
  };
}

export function formatSoakAudit(audit) {
  const kinds = Object.entries(audit.loadsByKind)
    .map(([kind, count]) => `${kind}=${count}`)
    .join(", ");
  const lines = [
    `Sessions: ${audit.startedSessions} started, ${audit.stoppedSnapshots} stopped snapshots, ${audit.failedSnapshots} failed snapshots`,
    `Inference loads: ${kinds}`,
    `Peak resident memory at load: ${formatMib(audit.peakResidentBytes)}`
  ];
  for (const profile of audit.profileMemory) {
    lines.push(
      `Post-stop ${profile.profile}: ${profile.samples} sample(s), first ${formatMib(profile.firstBytes)}, last ${formatMib(profile.lastBytes)}, growth ${formatMib(profile.growthBytes)}`
    );
  }
  lines.push(audit.ok ? "PASS: lifecycle log audit passed." : "FAIL: lifecycle log audit failed.");
  for (const failure of audit.failures) lines.push(`- ${failure}`);
  return lines.join("\n");
}

function summarizeProfileMemory(samples) {
  const groups = new Map();
  for (const sample of samples) {
    const group = groups.get(sample.profile) ?? [];
    group.push(sample.bytes);
    groups.set(sample.profile, group);
  }
  return [...groups.entries()].map(([profile, bytes]) => ({
    profile,
    samples: bytes.length,
    firstBytes: bytes[0] ?? 0,
    lastBytes: bytes.at(-1) ?? 0,
    peakBytes: Math.max(...bytes),
    growthBytes: Math.max(0, (bytes.at(-1) ?? 0) - (bytes[0] ?? 0))
  }));
}

function resourceFields(line) {
  const sessionId = numberField(line, "session_id");
  const kind = wordField(line, "kind");
  const modelId = stringField(line, "model_id");
  return sessionId > 0 && kind && modelId ? { sessionId, kind, modelId } : undefined;
}

function sessionKey(launchEpoch, sessionId) {
  return `${launchEpoch}:${sessionId}`;
}

function resourceKey(launchEpoch, resource) {
  return `${launchEpoch}:${resource.sessionId}:${resource.kind}:${resource.modelId}`;
}

function numberField(line, name) {
  const match = line.match(new RegExp(`(?:^|\\s)${name}=(\\d+)`, "u"));
  return match ? Number(match[1]) : 0;
}

function wordField(line, name) {
  return line.match(new RegExp(`(?:^|\\s)${name}=([A-Za-z]+)`, "u"))?.[1];
}

function stringField(line, name) {
  return line.match(new RegExp(`(?:^|\\s)${name}=([^\\s]+)`, "u"))?.[1];
}

function residentBytes(line) {
  const match = line.match(/(?:^|\s)resident_bytes=(?:Some\()?([0-9]+)/u);
  return match ? Number(match[1]) : 0;
}

function integerOption(value, fallback) {
  return Number.isSafeInteger(value) && value >= 0 ? value : fallback;
}

function formatMib(bytes) {
  return `${(bytes / MIB).toFixed(1)} MiB`;
}
