// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState, type CSSProperties } from "react";
import uiTheme from "@shared-ui-theme";
import {
  createLabFile,
  deleteLabFile,
  DRIVE_APPDATA_SCOPE,
  DRIVE_LAB_FILE_NAME,
  readLabFile,
  runDriveCasExperiment,
  runDriveCreateOnceExperiment,
  type DriveCasExperimentReport,
  type DriveCreateOnceExperimentReport,
  type DriveFileState,
  type ExperimentVerdict,
  type ModeExperimentResult,
  type RequestObservation,
} from "./driveCasApi";
import "./driveCasExperiment.css";

declare const __AEROBAG_GOOGLE_DRIVE_EXPERIMENT_CLIENT_ID__: string | null;

type GoogleTokenResponse = {
  access_token?: string;
  expires_in?: number | string;
  error?: string;
  error_description?: string;
};

type GoogleTokenClient = {
  requestAccessToken: (overrides?: { prompt?: string }) => void;
};

type GoogleTokenClientConfig = {
  client_id: string;
  scope: string;
  callback: (response: GoogleTokenResponse) => void;
  error_callback?: (error: { type?: string; message?: string }) => void;
};

declare global {
  interface Window {
    google?: {
      accounts: {
        oauth2: {
          initTokenClient: (config: GoogleTokenClientConfig) => GoogleTokenClient;
          revoke: (accessToken: string, callback?: () => void) => void;
        };
      };
    };
  }
}

type UiTheme = {
  controls: {
    button_checked: string;
    button_unchecked: string;
    button_disabled: string;
    button_fg: string;
    panel_bg: string;
    panel_border: string;
    panel_fg: string;
    panel_muted: string;
    data_status_warning_bg: string;
    data_status_warning_stroke: string;
    data_status_quiet_bg: string;
    data_status_quiet_stroke: string;
  };
};

const clientIdStorageKey = "aerobag.driveCasLab.clientId.v1";
const fileIdStorageKey = "aerobag.driveCasLab.fileId.v1";
const googleIdentityScriptId = "aerobag-google-identity-services";
const googleIdentityScriptUrl = "https://accounts.google.com/gsi/client";
const theme = uiTheme as UiTheme;

const themeVars = {
  "--lab-blue": theme.controls.button_checked,
  "--lab-blue-soft": theme.controls.button_unchecked,
  "--lab-disabled": theme.controls.button_disabled,
  "--lab-button-fg": theme.controls.button_fg,
  "--lab-paper": theme.controls.panel_bg,
  "--lab-border": theme.controls.panel_border,
  "--lab-ink": theme.controls.panel_fg,
  "--lab-muted": theme.controls.panel_muted,
  "--lab-warn-bg": theme.controls.data_status_warning_bg,
  "--lab-warn": theme.controls.data_status_warning_stroke,
  "--lab-quiet-bg": theme.controls.data_status_quiet_bg,
  "--lab-quiet": theme.controls.data_status_quiet_stroke,
} as CSSProperties;

function storedValue(key: string): string {
  try {
    return window.localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function persistValue(key: string, value: string) {
  try {
    if (value) {
      window.localStorage.setItem(key, value);
    } else {
      window.localStorage.removeItem(key);
    }
  } catch {
    // The experiment remains usable without persistence.
  }
}

function loadGoogleIdentityServices(): Promise<void> {
  if (window.google?.accounts.oauth2) {
    return Promise.resolve();
  }
  const existing = document.getElementById(googleIdentityScriptId) as HTMLScriptElement | null;
  if (existing) {
    return new Promise((resolve, reject) => {
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener("error", () => reject(new Error("Google Identity Services failed to load")), { once: true });
    });
  }
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.id = googleIdentityScriptId;
    script.src = googleIdentityScriptUrl;
    script.async = true;
    script.defer = true;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error("Google Identity Services failed to load"));
    document.head.append(script);
  });
}

function tokenLifetimeSeconds(response: GoogleTokenResponse): number {
  const parsed = Number(response.expires_in);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 3_600;
}

function googleAcceptsCurrentOrigin(): boolean {
  return window.location.protocol === "https:"
    || ["localhost", "127.0.0.1", "[::1]"].includes(window.location.hostname);
}

function downloadJson(filename: string, value: unknown) {
  const url = URL.createObjectURL(new Blob([`${JSON.stringify(value, null, 2)}\n`], { type: "application/json" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

async function uploadReport(
  report: DriveCasExperimentReport | DriveCreateOnceExperimentReport,
): Promise<void> {
  const response = await fetch("/__drive_cas_report", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(report),
  });
  if (!response.ok) {
    throw new Error(`Drive CAS evidence upload failed: HTTP ${response.status} ${await response.text()}`);
  }
}

function verdictLabel(verdict: ExperimentVerdict): string {
  if (verdict === "cas-observed") {
    return "CAS OBSERVED";
  }
  if (verdict === "unsafe") {
    return "UNSAFE";
  }
  return "INCONCLUSIVE";
}

function createOnceVerdictLabel(report: DriveCreateOnceExperimentReport): string {
  if (report.verdict === "create-once-observed") {
    return "ATOMIC CREATE-ONCE OBSERVED";
  }
  if (report.verdict === "unsafe") {
    return "UNSAFE";
  }
  return "INCONCLUSIVE";
}

function statusCode(outcome: { terminal_status: number | null }): string {
  return outcome.terminal_status === null ? "network" : String(outcome.terminal_status);
}

function ModeResult({ result }: { result: ModeExperimentResult }) {
  return (
    <section className={`driveCasModeResult is-${result.verdict}`}>
      <header>
        <h3>{result.mode}</h3>
        <span>{verdictLabel(result.verdict)}</span>
      </header>
      <p>
        Stale precondition: {statusCode(result.stale_precondition)}
        {result.stale_precondition_rejected ? " (rejected)" : " (not safely rejected)"}
      </p>
      <div className="driveCasRaceTable" role="table" aria-label={`${result.mode} race results`}>
        <div className="driveCasRaceRow isHeader" role="row">
          <span>Run</span>
          <span>Condition</span>
          <span>A</span>
          <span>B</span>
          <span>Final</span>
          <span>Verdict</span>
        </div>
        {result.iterations.map((iteration) => (
          <div className="driveCasRaceRow" role="row" key={iteration.iteration}>
            <span>{iteration.iteration}</span>
            <span title={iteration.condition.value}>{iteration.condition.source}</span>
            <span>{statusCode(iteration.writers[0])}</span>
            <span>{statusCode(iteration.writers[1])}</span>
            <span>{iteration.final_writer ?? "?"}</span>
            <span>{iteration.verdict}</span>
          </div>
        ))}
      </div>
      <details>
        <summary>Raw observations</summary>
        <pre>{JSON.stringify({
          stale_precondition: result.stale_precondition,
          iterations: result.iterations,
        }, null, 2)}</pre>
      </details>
    </section>
  );
}

function ObservationList({ observations }: { observations: RequestObservation[] }) {
  if (observations.length === 0) {
    return null;
  }
  return (
    <details className="driveCasObservations">
      <summary>Setup request evidence ({observations.length})</summary>
      <pre>{JSON.stringify(observations, null, 2)}</pre>
    </details>
  );
}

function CreateOnceResult({
  report,
  uploadStatus,
}: {
  report: DriveCreateOnceExperimentReport;
  uploadStatus: string;
}) {
  const observed = report.verdict === "create-once-observed";
  return (
    <section className="driveCasResults">
      <header className={observed ? "isGood" : ""}>
        <div>
          <p className="driveCasEyebrow">CREATE-ONCE REPORT</p>
          <h2>{createOnceVerdictLabel(report)}</h2>
          {uploadStatus ? <p>{uploadStatus}</p> : null}
        </div>
        <button
          type="button"
          className="isQuiet"
          onClick={() => downloadJson(`aerobag-drive-create-once-${Date.now()}.json`, report)}
        >
          Download JSON evidence
        </button>
      </header>

      <section className={`driveCasModeResult is-${observed ? "cas-observed" : report.verdict}`}>
        <header>
          <h3>Concurrent create races</h3>
          <span>{report.races.length} runs</span>
        </header>
        <div className="driveCasRaceTable" role="table" aria-label="Create-once race results">
          <div className="driveCasRaceRow isHeader" role="row">
            <span>Run</span>
            <span>Generated ID</span>
            <span>A</span>
            <span>B</span>
            <span>Final</span>
            <span>Verdict</span>
          </div>
          {report.races.map((race) => (
            <div className="driveCasRaceRow" role="row" key={race.iteration}>
              <span>{race.iteration}</span>
              <span title={race.file_id}>{race.file_id.slice(0, 8)}…</span>
              <span>{statusCode(race.writers[0])}</span>
              <span>{statusCode(race.writers[1])}</span>
              <span>{race.final_writer ?? "?"}</span>
              <span>{race.verdict}</span>
            </div>
          ))}
        </div>
      </section>

      <div className="driveCasColumns">
        <section className="driveCasModeResult">
          <header><h3>Retry same ID</h3></header>
          <p>
            First create: {statusCode(report.retry_after_success.first)} ·
            retry: {statusCode(report.retry_after_success.retry)}
          </p>
        </section>
        <section className="driveCasModeResult">
          <header><h3>Delete then reuse ID</h3></header>
          <p>
            First create: {statusCode(report.delete_then_recreate.first)} ·
            delete: {report.delete_then_recreate.deletion?.status ?? "not run"} ·
            recreate: {report.delete_then_recreate.recreate
              ? statusCode(report.delete_then_recreate.recreate)
              : "not run"}
          </p>
        </section>
      </div>

      <details>
        <summary>Raw create-once observations</summary>
        <pre>{JSON.stringify(report, null, 2)}</pre>
      </details>
    </section>
  );
}

export default function DriveCasExperiment() {
  const configuredClientId = __AEROBAG_GOOGLE_DRIVE_EXPERIMENT_CLIENT_ID__?.trim() ?? "";
  const [clientId, setClientId] = useState(() => storedValue(clientIdStorageKey) || configuredClientId);
  const [googleReady, setGoogleReady] = useState(false);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [tokenExpiresAt, setTokenExpiresAt] = useState<number | null>(null);
  const [fileId, setFileId] = useState(() => storedValue(fileIdStorageKey));
  const [fileState, setFileState] = useState<DriveFileState | null>(null);
  const [iterations, setIterations] = useState(3);
  const [payloadKib, setPayloadKib] = useState(256);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<DriveCasExperimentReport | null>(null);
  const [reportUploadStatus, setReportUploadStatus] = useState("");
  const [createOnceReport, setCreateOnceReport] = useState<DriveCreateOnceExperimentReport | null>(null);
  const [createOnceProgress, setCreateOnceProgress] = useState("");
  const [createOnceUploadStatus, setCreateOnceUploadStatus] = useState("");
  const [observations, setObservations] = useState<RequestObservation[]>([]);

  useEffect(() => {
    let cancelled = false;
    loadGoogleIdentityServices()
      .then(() => {
        if (!cancelled) {
          setGoogleReady(true);
        }
      })
      .catch((loadError) => {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : String(loadError));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function runTask(task: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (taskError) {
      setError(taskError instanceof Error ? taskError.message : String(taskError));
    } finally {
      setBusy(false);
      setProgress("");
      setCreateOnceProgress("");
    }
  }

  function authorize() {
    const trimmedClientId = clientId.trim();
    if (!trimmedClientId) {
      setError("Enter a Google OAuth web client ID.");
      return;
    }
    const oauth2 = window.google?.accounts.oauth2;
    if (!oauth2) {
      setError("Google Identity Services is not ready.");
      return;
    }
    setError(null);
    persistValue(clientIdStorageKey, trimmedClientId);
    const tokenClient = oauth2.initTokenClient({
      client_id: trimmedClientId,
      scope: DRIVE_APPDATA_SCOPE,
      callback: (response) => {
        if (!response.access_token || response.error) {
          setError(response.error_description ?? response.error ?? "Google did not return an access token.");
          return;
        }
        setAccessToken(response.access_token);
        setTokenExpiresAt(Date.now() + tokenLifetimeSeconds(response) * 1_000);
      },
      error_callback: (oauthError) => {
        setError(oauthError.message ?? oauthError.type ?? "Google authorization failed.");
      },
    });
    tokenClient.requestAccessToken({ prompt: "consent" });
  }

  function revokeAuthorization() {
    if (accessToken) {
      window.google?.accounts.oauth2.revoke(accessToken);
    }
    setAccessToken(null);
    setTokenExpiresAt(null);
    setFileState(null);
  }

  async function refreshFile() {
    if (!accessToken || !fileId) {
      return;
    }
    const nextState = await readLabFile(accessToken, fileId);
    setFileState(nextState);
    setObservations((current) => [...current, ...nextState.observations]);
  }

  const tokenIsCurrent = accessToken !== null && (tokenExpiresAt === null || Date.now() < tokenExpiresAt);
  const originIsAccepted = googleAcceptsCurrentOrigin();
  const canUseDrive = tokenIsCurrent && !busy;
  const allModesPassed = report?.modes.every((mode) => mode.verdict === "cas-observed") ?? false;

  return (
    <main className="driveCasLab" style={themeVars}>
      <header className="driveCasHero">
        <div>
          <p className="driveCasEyebrow">AEROBAG STORAGE EXPERIMENT 01</p>
          <h1>Can Drive do a real compare-and-swap?</h1>
          <p>
            This lab races conditional updates against one dedicated object in Google Drive&apos;s hidden
            <code> appDataFolder</code>. It never reads or modifies normal Drive files.
          </p>
        </div>
        <a href="/">Return to Aerobag</a>
      </header>

      <section className="driveCasWarning">
        <strong>Destructive lab boundary</strong>
        <span>
          The test repeatedly overwrites and may delete only <code>{DRIVE_LAB_FILE_NAME}</code>, created by this page.
          Do not point it at another file ID.
        </span>
      </section>

      <div className="driveCasColumns">
        <section className="driveCasCard">
          <header><span>01</span><h2>Authorize</h2></header>
          <label>
            Google OAuth web client ID
            <input
              value={clientId}
              onChange={(event) => setClientId(event.target.value)}
              placeholder="1234567890-….apps.googleusercontent.com"
              disabled={busy}
              spellCheck={false}
            />
          </label>
          <p className="driveCasHelp">
            Register <code>{window.location.origin}</code> as an authorized JavaScript origin. The requested scope is
            <code> drive.appdata</code>; no client secret belongs in this page.
          </p>
          {!originIsAccepted ? (
            <div className="driveCasError" role="alert">
              Google rejects plain-HTTP non-localhost OAuth origins. Reopen this lab through an
              <code> http://localhost</code> tunnel or a trusted HTTPS route.
            </div>
          ) : null}
          <div className="driveCasActions">
            <button type="button" onClick={authorize} disabled={!googleReady || busy || !originIsAccepted}>
              {accessToken ? "Reauthorize" : googleReady ? "Authorize Drive lab" : "Loading Google auth…"}
            </button>
            <button type="button" className="isQuiet" onClick={revokeAuthorization} disabled={!accessToken || busy}>
              Revoke token
            </button>
          </div>
          <div className={`driveCasStatus ${tokenIsCurrent ? "isGood" : ""}`}>
            {tokenIsCurrent
              ? `Authorized in memory; expires about ${new Date(tokenExpiresAt ?? Date.now()).toLocaleTimeString()}`
              : "No current Drive token"}
          </div>
        </section>

        <section className="driveCasCard">
          <header><span>02</span><h2>Lab object</h2></header>
          <p className="driveCasFileId">{fileId || "No lab object created"}</p>
          {fileState ? (
            <dl className="driveCasFacts">
              <div><dt>Drive version</dt><dd>{fileState.metadata.version}</dd></div>
              <div><dt>Condition source</dt><dd>{fileState.condition.source}</dd></div>
              <div><dt>Current writer</dt><dd>{fileState.payload?.writer ?? "empty"}</dd></div>
            </dl>
          ) : null}
          <div className="driveCasActions">
            <button
              type="button"
              disabled={!canUseDrive || Boolean(fileId)}
              onClick={() => void runTask(async () => {
                if (!accessToken) {
                  return;
                }
                const created = await createLabFile(accessToken);
                setFileId(created.file.id);
                persistValue(fileIdStorageKey, created.file.id);
                setObservations((current) => [...current, ...created.observations]);
                const nextState = await readLabFile(accessToken, created.file.id);
                setFileState(nextState);
                setObservations((current) => [...current, ...nextState.observations]);
              })}
            >
              Create fresh lab object
            </button>
            <button
              type="button"
              className="isQuiet"
              disabled={!canUseDrive || !fileId}
              onClick={() => void runTask(refreshFile)}
            >
              Refresh
            </button>
            <button
              type="button"
              className="isDanger"
              disabled={!canUseDrive || !fileId}
              onClick={() => void runTask(async () => {
                if (!accessToken || !fileId) {
                  return;
                }
                const deleted = await deleteLabFile(accessToken, fileId);
                setObservations((current) => [...current, deleted]);
                setFileId("");
                setFileState(null);
                setReport(null);
                setReportUploadStatus("");
                persistValue(fileIdStorageKey, "");
              })}
            >
              Delete lab object
            </button>
          </div>
        </section>
      </div>

      <section className="driveCasCard driveCasRunCard">
        <header><span>03</span><h2>Race it</h2></header>
        <div className="driveCasParameters">
          <label>
            Races per upload mode
            <input
              type="number"
              min={1}
              max={20}
              value={iterations}
              disabled={busy}
              onChange={(event) => setIterations(Math.max(1, Math.min(20, Number(event.target.value) || 1)))}
            />
          </label>
          <label>
            Payload KiB
            <input
              type="number"
              min={1}
              max={4_096}
              value={payloadKib}
              disabled={busy}
              onChange={(event) => setPayloadKib(Math.max(1, Math.min(4_096, Number(event.target.value) || 1)))}
            />
          </label>
          <button
            type="button"
            className="driveCasRunButton"
            disabled={!canUseDrive || !fileId}
            onClick={() => void runTask(async () => {
              if (!accessToken || !fileId) {
                return;
              }
              setReport(null);
              setReportUploadStatus("");
              const nextReport = await runDriveCasExperiment(accessToken, fileId, {
                iterations,
                payloadBytes: payloadKib * 1_024,
                onProgress: (nextProgress) => {
                  setProgress(nextProgress.phase === "stale-precondition"
                    ? `${nextProgress.mode}: testing a deliberately stale precondition`
                    : `${nextProgress.mode}: race ${nextProgress.iteration}/${nextProgress.total_iterations}`);
                },
              });
              setReport(nextReport);
              setReportUploadStatus("Saving evidence to the local Vite lab endpoint…");
              await uploadReport(nextReport);
              setReportUploadStatus("Evidence saved for direct inspection.");
              setFileState(await readLabFile(accessToken, fileId));
            })}
          >
            {busy ? "Experiment running…" : "Run all upload modes"}
          </button>
        </div>
        {progress ? <div className="driveCasProgress" role="status">{progress}</div> : null}
        {error ? <div className="driveCasError" role="alert">{error}</div> : null}
      </section>

      {report ? (
        <section className="driveCasResults">
          <header className={allModesPassed ? "isGood" : ""}>
            <div>
              <p className="driveCasEyebrow">EXPERIMENT REPORT</p>
              <h2>{allModesPassed ? "Conditional exclusion observed" : "Drive needs a closer look"}</h2>
              {reportUploadStatus ? <p>{reportUploadStatus}</p> : null}
            </div>
            <button
              type="button"
              className="isQuiet"
              onClick={() => downloadJson(`aerobag-drive-cas-${Date.now()}.json`, report)}
            >
              Download JSON evidence
            </button>
            <button
              type="button"
              className="isQuiet"
              disabled={busy}
              onClick={() => void runTask(async () => {
                setReportUploadStatus("Saving evidence to the local Vite lab endpoint…");
                await uploadReport(report);
                setReportUploadStatus("Evidence saved for direct inspection.");
              })}
            >
              Send evidence
            </button>
          </header>
          {report.modes.map((mode) => <ModeResult key={mode.mode} result={mode} />)}
        </section>
      ) : null}

      <section className="driveCasCard driveCasRunCard">
        <header><span>04</span><h2>Test generated-ID create-once</h2></header>
        <p className="driveCasHelp">
          Drive generates fresh <code>appDataFolder</code> file IDs. Each run races two creates using the
          same ID, retries one completed create, then tests whether a deleted ID can be reused.
        </p>
        <div className="driveCasActions">
          <button
            type="button"
            className="driveCasRunButton"
            disabled={!canUseDrive}
            onClick={() => void runTask(async () => {
              if (!accessToken) {
                return;
              }
              setCreateOnceReport(null);
              setCreateOnceUploadStatus("");
              const nextReport = await runDriveCreateOnceExperiment(accessToken, {
                iterations,
                payloadBytes: payloadKib * 1_024,
                onProgress: (nextProgress) => {
                  setCreateOnceProgress(
                    `create-once race ${nextProgress.iteration}/${nextProgress.total_iterations}`,
                  );
                },
              });
              setCreateOnceReport(nextReport);
              setCreateOnceProgress("");
              setCreateOnceUploadStatus("Saving evidence to the local Vite lab endpoint…");
              await uploadReport(nextReport);
              setCreateOnceUploadStatus("Evidence saved for direct inspection.");
            })}
          >
            {busy ? "Experiment running…" : "Run create-once experiment"}
          </button>
        </div>
        {createOnceProgress ? <div className="driveCasProgress" role="status">{createOnceProgress}</div> : null}
      </section>

      {createOnceReport ? (
        <CreateOnceResult report={createOnceReport} uploadStatus={createOnceUploadStatus} />
      ) : null}

      <ObservationList observations={observations} />
    </main>
  );
}
