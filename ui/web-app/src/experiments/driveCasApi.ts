// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

const DRIVE_API_ROOT = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_ROOT = "https://www.googleapis.com/upload/drive/v3";
const LAB_MIME_TYPE = "application/vnd.aerobag.drive-cas-lab+json";
const FILE_FIELDS = "id,name,version,md5Checksum,sha256Checksum,modifiedTime,headRevisionId,size,trashed";

export const DRIVE_APPDATA_SCOPE = "https://www.googleapis.com/auth/drive.appdata";
export const DRIVE_LAB_FILE_NAME = "aerobag-drive-cas-lab-v1";

export type UploadMode = "simple" | "multipart" | "resumable";
export type ExperimentVerdict = "cas-observed" | "unsafe" | "inconclusive";
export type RaceVerdict = "single-winner" | "multiple-winners" | "inconclusive";

export type RequestObservation = {
  label: string;
  method: string;
  url: string;
  status: number | null;
  ok: boolean;
  elapsed_ms: number;
  response_headers: Record<string, string>;
  response_body_preview: string;
  response_body_bytes: number;
  error: string | null;
};

export type DriveFileMetadata = {
  id: string;
  name: string;
  version: string;
  md5Checksum?: string;
  sha256Checksum?: string;
  modifiedTime?: string;
  headRevisionId?: string;
  size?: string;
  trashed?: boolean;
};

export type LabPayload = {
  schema: "aerobag-drive-cas-lab-v1";
  run_id: string;
  mode: UploadMode | "setup";
  writer: string;
  iteration: number;
  created_at: string;
  pad: string;
};

export type DriveFileState = {
  metadata: DriveFileMetadata;
  payload: LabPayload | null;
  condition: {
    value: string;
    source: "http-etag" | "quoted-drive-version";
  };
  observations: RequestObservation[];
};

export type WriteOutcome = {
  writer: string;
  success: boolean;
  conflict: boolean;
  terminal_status: number | null;
  observations: RequestObservation[];
};

export type RaceIterationResult = {
  iteration: number;
  condition: DriveFileState["condition"];
  writers: [WriteOutcome, WriteOutcome];
  final_writer: string | null;
  verdict: RaceVerdict;
  observations: RequestObservation[];
};

export type ModeExperimentResult = {
  mode: UploadMode;
  stale_precondition: WriteOutcome;
  stale_precondition_rejected: boolean;
  iterations: RaceIterationResult[];
  verdict: ExperimentVerdict;
};

export type DriveCasExperimentReport = {
  schema: "aerobag-drive-cas-report-v1";
  started_at: string;
  completed_at: string;
  origin: string;
  user_agent: string;
  file_id: string;
  iterations_per_mode: number;
  payload_bytes: number;
  modes: ModeExperimentResult[];
};

export type CreateOnceExperimentVerdict = "create-once-observed" | "unsafe" | "inconclusive";

export type CreateOncePayload = {
  schema: "aerobag-drive-create-once-lab-v1";
  run_id: string;
  writer: string;
  iteration: number;
  created_at: string;
  pad: string;
};

export type CreateOnceRaceResult = {
  iteration: number;
  file_id: string;
  writers: [WriteOutcome, WriteOutcome];
  final_writer: string | null;
  verdict: RaceVerdict;
  observations: RequestObservation[];
  cleanup: RequestObservation | null;
};

export type CreateOnceRetryResult = {
  file_id: string;
  first: WriteOutcome;
  retry: WriteOutcome;
  final_writer: string | null;
  observations: RequestObservation[];
  cleanup: RequestObservation | null;
};

export type CreateOnceDeleteReuseResult = {
  file_id: string;
  first: WriteOutcome;
  deletion: RequestObservation | null;
  recreate: WriteOutcome | null;
  cleanup: RequestObservation | null;
};

export type DriveCreateOnceExperimentReport = {
  schema: "aerobag-drive-create-once-report-v1";
  started_at: string;
  completed_at: string;
  origin: string;
  user_agent: string;
  iterations: number;
  payload_bytes: number;
  setup_observations: RequestObservation[];
  races: CreateOnceRaceResult[];
  retry_after_success: CreateOnceRetryResult;
  delete_then_recreate: CreateOnceDeleteReuseResult;
  verdict: CreateOnceExperimentVerdict;
};

type ObservedResponse = {
  response: Response | null;
  text: string;
  observation: RequestObservation;
};

type ModeProgress = {
  mode: UploadMode;
  phase: "stale-precondition" | "race";
  iteration: number;
  total_iterations: number;
};

export type ExperimentOptions = {
  iterations: number;
  payloadBytes: number;
  onProgress?: (progress: ModeProgress) => void;
};

type ResumableStart = {
  writer: string;
  payloadText: string;
  uploadUrl: string | null;
  observations: RequestObservation[];
};

type ReadFilePayloadResult<T> = {
  metadata: DriveFileMetadata;
  payload: T | null;
  metadataResponse: ObservedResponse;
  mediaResponse: ObservedResponse;
  observations: RequestObservation[];
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function redactObservationUrl(value: string): string {
  try {
    const url = new URL(value);
    let redacted = false;
    const keys = [...url.searchParams.keys()];
    const carriesUploadSession = keys.some((key) => /^(upload_?id|session_crd|x-goog-upload-id)$/i.test(key));
    for (const key of keys) {
      if (
        /^(upload_?id|session_crd|x-goog-upload-id)$/i.test(key)
        || (carriesUploadSession && !/^(uploadType|fields)$/i.test(key))
      ) {
        url.searchParams.set(key, "<redacted>");
        redacted = true;
      }
    }
    return redacted ? url.toString() : value;
  } catch {
    return "<invalid-url-redacted>";
  }
}

function responseHeaders(response: Response): Record<string, string> {
  return Object.fromEntries(
    Array.from(response.headers.entries())
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([name, value]) => {
        if (name === "location") {
          return [name, redactObservationUrl(value)];
        }
        if (/upload-?id/i.test(name)) {
          return [name, "<redacted>"];
        }
        return [name, value];
      }),
  );
}

async function observedFetch(label: string, url: string, init: RequestInit): Promise<ObservedResponse> {
  const startedAt = performance.now();
  try {
    const response = await fetch(url, init);
    const text = await response.text();
    return {
      response,
      text,
      observation: {
        label,
        method: init.method ?? "GET",
        url: redactObservationUrl(url),
        status: response.status,
        ok: response.ok,
        elapsed_ms: Math.round(performance.now() - startedAt),
        response_headers: responseHeaders(response),
        response_body_preview: text.slice(0, 2_000),
        response_body_bytes: new TextEncoder().encode(text).byteLength,
        error: null,
      },
    };
  } catch (error) {
    return {
      response: null,
      text: "",
      observation: {
        label,
        method: init.method ?? "GET",
        url: redactObservationUrl(url),
        status: null,
        ok: false,
        elapsed_ms: Math.round(performance.now() - startedAt),
        response_headers: {},
        response_body_preview: "",
        response_body_bytes: 0,
        error: errorMessage(error),
      },
    };
  }
}

function authorizationHeaders(accessToken: string, additional?: HeadersInit): Headers {
  const headers = new Headers(additional);
  headers.set("Authorization", `Bearer ${accessToken}`);
  return headers;
}

function parseJson<T>(text: string, description: string): T {
  try {
    return JSON.parse(text) as T;
  } catch (error) {
    throw new Error(`${description} was not valid JSON: ${errorMessage(error)}`);
  }
}

function requireSuccessfulResponse(result: ObservedResponse, description: string): Response {
  if (!result.response?.ok) {
    const detail = result.observation.error
      ?? result.observation.response_body_preview
      ?? `HTTP ${result.observation.status ?? "network error"}`;
    throw new Error(`${description} failed: ${detail}`);
  }
  return result.response;
}

function conditionForState(
  metadata: DriveFileMetadata,
  metadataResponse: ObservedResponse,
  mediaResponse: ObservedResponse,
): DriveFileState["condition"] {
  const etag = metadataResponse.response?.headers.get("etag")
    ?? mediaResponse.response?.headers.get("etag");
  if (etag) {
    return { value: etag, source: "http-etag" };
  }
  return {
    value: `"${metadata.version}"`,
    source: "quoted-drive-version",
  };
}

export async function createLabFile(accessToken: string): Promise<{
  file: DriveFileMetadata;
  observations: RequestObservation[];
}> {
  const createResult = await observedFetch(
    "create lab file metadata",
    `${DRIVE_API_ROOT}/files?fields=${encodeURIComponent(FILE_FIELDS)}`,
    {
      method: "POST",
      headers: authorizationHeaders(accessToken, { "Content-Type": "application/json" }),
      body: JSON.stringify({
        name: DRIVE_LAB_FILE_NAME,
        mimeType: LAB_MIME_TYPE,
        parents: ["appDataFolder"],
      }),
    },
  );
  requireSuccessfulResponse(createResult, "Drive lab file creation");
  const file = parseJson<DriveFileMetadata>(createResult.text, "Drive lab file metadata");
  const setupPayload = makePayload("setup", "setup", 0, 0);
  const setupWrite = await writePayload(accessToken, file.id, "simple", setupPayload, null);
  if (!setupWrite.success) {
    throw new Error(`Drive lab file setup write failed: HTTP ${setupWrite.terminal_status ?? "network error"}`);
  }
  return {
    file,
    observations: [createResult.observation, ...setupWrite.observations],
  };
}

export async function deleteLabFile(accessToken: string, fileId: string): Promise<RequestObservation> {
  return deleteDriveFile(accessToken, fileId, "delete lab file");
}

async function deleteDriveFile(
  accessToken: string,
  fileId: string,
  label: string,
): Promise<RequestObservation> {
  const result = await observedFetch(
    label,
    `${DRIVE_API_ROOT}/files/${encodeURIComponent(fileId)}`,
    {
      method: "DELETE",
      headers: authorizationHeaders(accessToken),
    },
  );
  if (!result.response?.ok && result.response?.status !== 404) {
    requireSuccessfulResponse(result, "Drive lab file deletion");
  }
  return result.observation;
}

async function readFilePayload<T>(
  accessToken: string,
  fileId: string,
  payloadDescription: string,
  labelPrefix = "read lab file",
): Promise<ReadFilePayloadResult<T>> {
  const encodedFileId = encodeURIComponent(fileId);
  const [metadataResult, mediaResult] = await Promise.all([
    observedFetch(
      `${labelPrefix} metadata`,
      `${DRIVE_API_ROOT}/files/${encodedFileId}?fields=${encodeURIComponent(FILE_FIELDS)}`,
      {
        method: "GET",
        headers: authorizationHeaders(accessToken),
      },
    ),
    observedFetch(
      `${labelPrefix} content`,
      `${DRIVE_API_ROOT}/files/${encodedFileId}?alt=media`,
      {
        method: "GET",
        headers: authorizationHeaders(accessToken),
      },
    ),
  ]);
  requireSuccessfulResponse(metadataResult, "Drive lab metadata read");
  requireSuccessfulResponse(mediaResult, "Drive lab content read");
  const metadata = parseJson<DriveFileMetadata>(metadataResult.text, "Drive lab metadata");
  let payload: T | null = null;
  if (mediaResult.text.trim()) {
    payload = parseJson<T>(mediaResult.text, payloadDescription);
  }
  return {
    metadata,
    payload,
    metadataResponse: metadataResult,
    mediaResponse: mediaResult,
    observations: [metadataResult.observation, mediaResult.observation],
  };
}

export async function readLabFile(accessToken: string, fileId: string): Promise<DriveFileState> {
  const result = await readFilePayload<LabPayload>(accessToken, fileId, "Drive lab payload");
  return {
    metadata: result.metadata,
    payload: result.payload,
    condition: conditionForState(result.metadata, result.metadataResponse, result.mediaResponse),
    observations: result.observations,
  };
}

function makePayload(
  mode: UploadMode | "setup",
  writer: string,
  iteration: number,
  payloadBytes: number,
): LabPayload {
  const runId = crypto.randomUUID();
  return {
    schema: "aerobag-drive-cas-lab-v1",
    run_id: runId,
    mode,
    writer,
    iteration,
    created_at: new Date().toISOString(),
    pad: writer.slice(0, 1).repeat(Math.max(0, payloadBytes)),
  };
}

function multipartBody(
  payload: unknown,
  metadata: Record<string, unknown> = {},
): { body: string; contentType: string } {
  const boundary = `aerobag_drive_cas_${crypto.randomUUID().replaceAll("-", "")}`;
  const body = [
    `--${boundary}`,
    "Content-Type: application/json; charset=UTF-8",
    "",
    JSON.stringify(metadata),
    `--${boundary}`,
    `Content-Type: ${LAB_MIME_TYPE}`,
    "",
    JSON.stringify(payload),
    `--${boundary}--`,
    "",
  ].join("\r\n");
  return {
    body,
    contentType: `multipart/related; boundary=${boundary}`,
  };
}

function terminalWriteOutcome(writer: string, observations: RequestObservation[]): WriteOutcome {
  const terminal = observations.at(-1);
  const status = terminal?.status ?? null;
  return {
    writer,
    success: terminal?.ok ?? false,
    conflict: status === 409 || status === 412,
    terminal_status: status,
    observations,
  };
}

async function writePayload(
  accessToken: string,
  fileId: string,
  mode: UploadMode,
  payload: LabPayload,
  ifMatch: string | null,
): Promise<WriteOutcome> {
  const encodedFileId = encodeURIComponent(fileId);
  const conditionHeaders: HeadersInit = ifMatch ? { "If-Match": ifMatch } : {};
  if (mode === "simple") {
    const result = await observedFetch(
      `${payload.writer} simple upload`,
      `${DRIVE_UPLOAD_ROOT}/files/${encodedFileId}?uploadType=media&fields=${encodeURIComponent(FILE_FIELDS)}`,
      {
        method: "PATCH",
        headers: authorizationHeaders(accessToken, {
          ...conditionHeaders,
          "Content-Type": LAB_MIME_TYPE,
        }),
        body: JSON.stringify(payload),
      },
    );
    return terminalWriteOutcome(payload.writer, [result.observation]);
  }

  if (mode === "multipart") {
    const multipart = multipartBody(payload);
    const result = await observedFetch(
      `${payload.writer} multipart upload`,
      `${DRIVE_UPLOAD_ROOT}/files/${encodedFileId}?uploadType=multipart&fields=${encodeURIComponent(FILE_FIELDS)}`,
      {
        method: "PATCH",
        headers: authorizationHeaders(accessToken, {
          ...conditionHeaders,
          "Content-Type": multipart.contentType,
        }),
        body: multipart.body,
      },
    );
    return terminalWriteOutcome(payload.writer, [result.observation]);
  }

  const start = await startResumableUpload(accessToken, fileId, payload, ifMatch);
  return finishResumableUpload(accessToken, start);
}

async function startResumableUpload(
  accessToken: string,
  fileId: string,
  payload: LabPayload,
  ifMatch: string | null,
): Promise<ResumableStart> {
  const encodedFileId = encodeURIComponent(fileId);
  const conditionHeaders: Record<string, string> = ifMatch ? { "If-Match": ifMatch } : {};
  const payloadText = JSON.stringify(payload);
  const startResult = await observedFetch(
    `${payload.writer} resumable start`,
    `${DRIVE_UPLOAD_ROOT}/files/${encodedFileId}?uploadType=resumable&fields=${encodeURIComponent(FILE_FIELDS)}`,
    {
      method: "PATCH",
      headers: authorizationHeaders(accessToken, {
        ...conditionHeaders,
        "Content-Type": "application/json; charset=UTF-8",
        "X-Upload-Content-Type": LAB_MIME_TYPE,
        "X-Upload-Content-Length": String(new TextEncoder().encode(payloadText).byteLength),
      }),
      body: "{}",
    },
  );
  if (!startResult.response?.ok) {
    return {
      writer: payload.writer,
      payloadText,
      uploadUrl: null,
      observations: [startResult.observation],
    };
  }
  const uploadUrl = startResult.response.headers.get("location");
  if (!uploadUrl) {
    const missingLocation = {
      ...startResult.observation,
      ok: false,
      error: "successful resumable-upload start did not expose a Location header",
    };
    return {
      writer: payload.writer,
      payloadText,
      uploadUrl: null,
      observations: [missingLocation],
    };
  }
  return {
    writer: payload.writer,
    payloadText,
    uploadUrl,
    observations: [startResult.observation],
  };
}

async function finishResumableUpload(
  accessToken: string,
  start: ResumableStart,
): Promise<WriteOutcome> {
  if (!start.uploadUrl) {
    return terminalWriteOutcome(start.writer, start.observations);
  }
  const finishResult = await observedFetch(
    `${start.writer} resumable finish`,
    start.uploadUrl,
    {
      method: "PUT",
      headers: authorizationHeaders(accessToken, {
        "Content-Type": LAB_MIME_TYPE,
      }),
      body: start.payloadText,
    },
  );
  return terminalWriteOutcome(start.writer, [...start.observations, finishResult.observation]);
}

async function racePayloads(
  accessToken: string,
  fileId: string,
  mode: UploadMode,
  payloadA: LabPayload,
  payloadB: LabPayload,
  condition: string,
): Promise<[WriteOutcome, WriteOutcome]> {
  if (mode !== "resumable") {
    return Promise.all([
      writePayload(accessToken, fileId, mode, payloadA, condition),
      writePayload(accessToken, fileId, mode, payloadB, condition),
    ]) as Promise<[WriteOutcome, WriteOutcome]>;
  }

  // Establish both resumable sessions before either uploads data. This catches
  // providers that check a precondition only when creating the session, then
  // allow two sessions based on the same old revision to commit.
  const starts = await Promise.all([
    startResumableUpload(accessToken, fileId, payloadA, condition),
    startResumableUpload(accessToken, fileId, payloadB, condition),
  ]) as [ResumableStart, ResumableStart];
  return Promise.all(starts.map((start) => finishResumableUpload(accessToken, start))) as Promise<
    [WriteOutcome, WriteOutcome]
  >;
}

export function classifyRace(writers: readonly WriteOutcome[], finalWriter: string | null): RaceVerdict {
  const successful = writers.filter((writer) => writer.success);
  if (successful.length > 1) {
    return "multiple-winners";
  }
  if (
    successful.length === 1
    && writers.some((writer) => writer.conflict)
    && finalWriter === successful[0]?.writer
  ) {
    return "single-winner";
  }
  return "inconclusive";
}

async function resetFile(
  accessToken: string,
  fileId: string,
  mode: UploadMode,
  iteration: number,
  payloadBytes: number,
): Promise<DriveFileState> {
  const resetPayload = makePayload(mode, `BASE-${mode}-${iteration}`, iteration, payloadBytes);
  const reset = await writePayload(accessToken, fileId, "simple", resetPayload, null);
  if (!reset.success) {
    throw new Error(`Failed to reset Drive lab file before ${mode} iteration ${iteration}`);
  }
  const state = await readLabFile(accessToken, fileId);
  state.observations.unshift(...reset.observations);
  return state;
}

export async function runModeExperiment(
  accessToken: string,
  fileId: string,
  mode: UploadMode,
  options: ExperimentOptions,
): Promise<ModeExperimentResult> {
  options.onProgress?.({
    mode,
    phase: "stale-precondition",
    iteration: 0,
    total_iterations: options.iterations,
  });
  await resetFile(accessToken, fileId, mode, 0, options.payloadBytes);
  const stalePayload = makePayload(mode, `STALE-${mode}`, 0, options.payloadBytes);
  const staleCondition = `"aerobag-stale-${crypto.randomUUID()}"`;
  const stalePrecondition = await writePayload(accessToken, fileId, mode, stalePayload, staleCondition);
  const stalePreconditionRejected = stalePrecondition.conflict;

  const iterations: RaceIterationResult[] = [];
  for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
    options.onProgress?.({
      mode,
      phase: "race",
      iteration,
      total_iterations: options.iterations,
    });
    const baseState = await resetFile(accessToken, fileId, mode, iteration, options.payloadBytes);
    const writerA = makePayload(mode, `A-${mode}-${iteration}`, iteration, options.payloadBytes);
    const writerB = makePayload(mode, `B-${mode}-${iteration}`, iteration, options.payloadBytes);
    const writers = await racePayloads(
      accessToken,
      fileId,
      mode,
      writerA,
      writerB,
      baseState.condition.value,
    );
    const finalState = await readLabFile(accessToken, fileId);
    const finalWriter = finalState.payload?.writer ?? null;
    iterations.push({
      iteration,
      condition: baseState.condition,
      writers,
      final_writer: finalWriter,
      verdict: classifyRace(writers, finalWriter),
      observations: [
        ...baseState.observations,
        ...writers.flatMap((writer) => writer.observations),
        ...finalState.observations,
      ],
    });
  }

  const raceVerdicts = iterations.map((iteration) => iteration.verdict);
  const verdict: ExperimentVerdict = !stalePreconditionRejected || raceVerdicts.includes("multiple-winners")
    ? "unsafe"
    : raceVerdicts.every((raceVerdict) => raceVerdict === "single-winner")
      ? "cas-observed"
      : "inconclusive";
  return {
    mode,
    stale_precondition: stalePrecondition,
    stale_precondition_rejected: stalePreconditionRejected,
    iterations,
    verdict,
  };
}

export async function runDriveCasExperiment(
  accessToken: string,
  fileId: string,
  options: ExperimentOptions,
): Promise<DriveCasExperimentReport> {
  const startedAt = new Date().toISOString();
  const modes: ModeExperimentResult[] = [];
  for (const mode of ["simple", "multipart", "resumable"] as const) {
    modes.push(await runModeExperiment(accessToken, fileId, mode, options));
  }
  return {
    schema: "aerobag-drive-cas-report-v1",
    started_at: startedAt,
    completed_at: new Date().toISOString(),
    origin: window.location.origin,
    user_agent: navigator.userAgent,
    file_id: fileId,
    iterations_per_mode: options.iterations,
    payload_bytes: options.payloadBytes,
    modes,
  };
}

type GeneratedIdsResponse = {
  ids?: string[];
  space?: string;
};

async function generateAppDataFileIds(
  accessToken: string,
  count: number,
): Promise<{ ids: string[]; observation: RequestObservation }> {
  const result = await observedFetch(
    "generate appDataFolder file IDs",
    `${DRIVE_API_ROOT}/files/generateIds?count=${count}&space=appDataFolder&type=files`,
    {
      method: "GET",
      headers: authorizationHeaders(accessToken),
    },
  );
  requireSuccessfulResponse(result, "Drive generated-ID request");
  const response = parseJson<GeneratedIdsResponse>(result.text, "Drive generated IDs");
  if (response.space !== "appDataFolder" || response.ids?.length !== count) {
    throw new Error(
      `Drive returned ${response.ids?.length ?? 0} IDs for ${response.space ?? "unknown space"}; expected ${count} appDataFolder IDs`,
    );
  }
  return { ids: response.ids, observation: result.observation };
}

function makeCreateOncePayload(
  writer: string,
  iteration: number,
  payloadBytes: number,
): CreateOncePayload {
  return {
    schema: "aerobag-drive-create-once-lab-v1",
    run_id: crypto.randomUUID(),
    writer,
    iteration,
    created_at: new Date().toISOString(),
    pad: writer.slice(0, 1).repeat(Math.max(0, payloadBytes)),
  };
}

async function createWithGeneratedId(
  accessToken: string,
  fileId: string,
  payload: CreateOncePayload,
): Promise<WriteOutcome> {
  const multipart = multipartBody(payload, {
    id: fileId,
    name: `aerobag-drive-create-once-${payload.run_id}`,
    mimeType: LAB_MIME_TYPE,
    parents: ["appDataFolder"],
  });
  const result = await observedFetch(
    `${payload.writer} create generated ID`,
    `${DRIVE_UPLOAD_ROOT}/files?uploadType=multipart&fields=${encodeURIComponent(FILE_FIELDS)}`,
    {
      method: "POST",
      headers: authorizationHeaders(accessToken, {
        "Content-Type": multipart.contentType,
      }),
      body: multipart.body,
    },
  );
  return terminalWriteOutcome(payload.writer, [result.observation]);
}

async function readCreateOnceWriter(
  accessToken: string,
  fileId: string,
): Promise<{ writer: string | null; observations: RequestObservation[] }> {
  const state = await readFilePayload<CreateOncePayload>(
    accessToken,
    fileId,
    "Drive create-once payload",
    "read create-once file",
  );
  return {
    writer: state.payload?.writer ?? null,
    observations: state.observations,
  };
}

export function classifyCreateOnceExperiment(
  raceVerdicts: readonly RaceVerdict[],
  firstCreate: WriteOutcome,
  retryCreate: WriteOutcome,
): CreateOnceExperimentVerdict {
  if (raceVerdicts.includes("multiple-winners") || retryCreate.success) {
    return "unsafe";
  }
  if (
    raceVerdicts.length > 0
    && raceVerdicts.every((verdict) => verdict === "single-winner")
    && firstCreate.success
    && retryCreate.conflict
  ) {
    return "create-once-observed";
  }
  return "inconclusive";
}

export async function runDriveCreateOnceExperiment(
  accessToken: string,
  options: ExperimentOptions,
): Promise<DriveCreateOnceExperimentReport> {
  const startedAt = new Date().toISOString();
  const generated = await generateAppDataFileIds(accessToken, options.iterations + 2);
  const races: CreateOnceRaceResult[] = [];

  for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
    options.onProgress?.({
      mode: "multipart",
      phase: "race",
      iteration,
      total_iterations: options.iterations,
    });
    const fileId = generated.ids[iteration - 1]!;
    const payloadA = makeCreateOncePayload(`A-create-once-${iteration}`, iteration, options.payloadBytes);
    const payloadB = makeCreateOncePayload(`B-create-once-${iteration}`, iteration, options.payloadBytes);
    const writers = await Promise.all([
      createWithGeneratedId(accessToken, fileId, payloadA),
      createWithGeneratedId(accessToken, fileId, payloadB),
    ]) as [WriteOutcome, WriteOutcome];
    const final = writers.some((writer) => writer.success)
      ? await readCreateOnceWriter(accessToken, fileId)
      : { writer: null, observations: [] };
    const verdict = classifyRace(writers, final.writer);
    const cleanup = writers.some((writer) => writer.success)
      ? await deleteDriveFile(accessToken, fileId, `cleanup create-once race ${iteration}`)
      : null;
    races.push({
      iteration,
      file_id: fileId,
      writers,
      final_writer: final.writer,
      verdict,
      observations: [...writers.flatMap((writer) => writer.observations), ...final.observations],
      cleanup,
    });
  }

  const retryFileId = generated.ids[options.iterations]!;
  const retryPayload = makeCreateOncePayload("RETRY-create-once", 0, options.payloadBytes);
  const firstCreate = await createWithGeneratedId(accessToken, retryFileId, retryPayload);
  const retryCreate = await createWithGeneratedId(accessToken, retryFileId, retryPayload);
  const retryFinal = firstCreate.success
    ? await readCreateOnceWriter(accessToken, retryFileId)
    : { writer: null, observations: [] };
  const retryCleanup = firstCreate.success
    ? await deleteDriveFile(accessToken, retryFileId, "cleanup create-once retry")
    : null;
  const retryAfterSuccess: CreateOnceRetryResult = {
    file_id: retryFileId,
    first: firstCreate,
    retry: retryCreate,
    final_writer: retryFinal.writer,
    observations: [
      ...firstCreate.observations,
      ...retryCreate.observations,
      ...retryFinal.observations,
    ],
    cleanup: retryCleanup,
  };

  const deleteReuseFileId = generated.ids[options.iterations + 1]!;
  const deleteReuseFirst = await createWithGeneratedId(
    accessToken,
    deleteReuseFileId,
    makeCreateOncePayload("DELETE-REUSE-first", 0, options.payloadBytes),
  );
  const deletion = deleteReuseFirst.success
    ? await deleteDriveFile(accessToken, deleteReuseFileId, "delete create-once lifecycle file")
    : null;
  const recreate = deletion?.ok
    ? await createWithGeneratedId(
      accessToken,
      deleteReuseFileId,
      makeCreateOncePayload("DELETE-REUSE-second", 1, options.payloadBytes),
    )
    : null;
  const lifecycleCleanup = recreate?.success
    ? await deleteDriveFile(accessToken, deleteReuseFileId, "cleanup recreated create-once lifecycle file")
    : null;

  return {
    schema: "aerobag-drive-create-once-report-v1",
    started_at: startedAt,
    completed_at: new Date().toISOString(),
    origin: window.location.origin,
    user_agent: navigator.userAgent,
    iterations: options.iterations,
    payload_bytes: options.payloadBytes,
    setup_observations: [generated.observation],
    races,
    retry_after_success: retryAfterSuccess,
    delete_then_recreate: {
      file_id: deleteReuseFileId,
      first: deleteReuseFirst,
      deletion,
      recreate,
      cleanup: lifecycleCleanup,
    },
    verdict: classifyCreateOnceExperiment(
      races.map((race) => race.verdict),
      firstCreate,
      retryCreate,
    ),
  };
}
