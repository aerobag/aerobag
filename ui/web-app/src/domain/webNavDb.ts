import sqlite3InitModule, {
  type BindingSpec,
  type Database,
  type SqlValue,
  type Sqlite3Static,
} from "@sqlite.org/sqlite-wasm";
import { debugLog } from "./debugLog";

const DEFAULT_NAV_DB_URL = "/nav-db/main.db";
const DEFAULT_DB_FILENAME = "/nav-main.db";
const IGNORED_SQLITE_INIT_WARNINGS = [
  "Ignoring inability to install OPFS sqlite3_vfs",
];
const sqlite3InitWithConfig = sqlite3InitModule as unknown as (config: {
  printErr: (message: unknown) => void;
}) => Promise<Sqlite3Static>;

export class BrowserNavDb {
  private constructor(
    readonly sqlite3: Sqlite3Static,
    readonly db: Database,
    readonly sourceUrl: string,
  ) {}

  static async open(sourceUrl = DEFAULT_NAV_DB_URL): Promise<BrowserNavDb> {
    const startMs = performance.now();
    debugLog("navdb.open.start", { sourceUrl });
    const sqlite3 = await sqlite3InitWithConfig({
      printErr(message) {
        const rendered = String(message);
        if (IGNORED_SQLITE_INIT_WARNINGS.some((warning) => rendered.includes(warning))) {
          return;
        }
        console.error(rendered);
      },
    });
    debugLog("navdb.open.sqlite_ready", { elapsed_ms: Math.round(performance.now() - startMs) });
    const response = await fetch(sourceUrl);
    if (!response.ok) {
      debugLog("navdb.open.fetch_failed", {
        sourceUrl,
        status: response.status,
        status_text: response.statusText,
        elapsed_ms: Math.round(performance.now() - startMs),
      });
      throw new Error(`failed to fetch nav db ${sourceUrl}: ${response.status} ${response.statusText}`);
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    debugLog("navdb.open.fetched", {
      bytes: bytes.byteLength,
      elapsed_ms: Math.round(performance.now() - startMs),
    });
    sqlite3.capi.sqlite3_js_posix_create_file(DEFAULT_DB_FILENAME, bytes);
    const db = new sqlite3.oo1.DB(DEFAULT_DB_FILENAME, "r");
    debugLog("navdb.open.ready", { elapsed_ms: Math.round(performance.now() - startMs) });
    return new BrowserNavDb(sqlite3, db, sourceUrl);
  }

  queryObjects<T extends Record<string, SqlValue>>(
    sql: string,
    bind?: BindingSpec,
  ): T[] {
    return this.db.exec({
      sql,
      bind,
      rowMode: "object",
      returnValue: "resultRows",
    }) as T[];
  }

  queryScalars<T extends SqlValue>(sql: string, bind?: BindingSpec): T[] {
    return this.db.exec({
      sql,
      bind,
      rowMode: 0,
      returnValue: "resultRows",
    }) as T[];
  }

  queryScalar<T extends SqlValue>(sql: string, bind?: BindingSpec): T | undefined {
    return this.queryScalars<T>(sql, bind)[0];
  }

  close(): void {
    this.db.close();
  }
}

let sharedNavDbPromise: Promise<BrowserNavDb> | null = null;

export function getBrowserNavDb(sourceUrl = DEFAULT_NAV_DB_URL): Promise<BrowserNavDb> {
  if (sharedNavDbPromise === null) {
    sharedNavDbPromise = BrowserNavDb.open(sourceUrl);
  }
  return sharedNavDbPromise;
}

declare global {
  // Core/WASM owns all nav-db SQL and interpretation. This hook is deliberately
  // a dumb web SQLite transport, not an application-layer planning API.
  // eslint-disable-next-line no-var
  var __aerobagNavDbQueryObjects: ((sql: string, bindJson: string) => string) | undefined;
}

export async function installBrowserNavDbQueryHost(sourceUrl = DEFAULT_NAV_DB_URL): Promise<void> {
  const db = await getBrowserNavDb(sourceUrl);
  globalThis.__aerobagNavDbQueryObjects = (sql: string, bindJson: string) => {
    const bind = JSON.parse(bindJson) as BindingSpec | undefined;
    return JSON.stringify(db.queryObjects(sql, bind));
  };
}
