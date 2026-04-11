import sqlite3InitModule, {
  type BindingSpec,
  type Database,
  type SqlValue,
  type Sqlite3Static,
} from "@sqlite.org/sqlite-wasm";

const DEFAULT_NAV_DB_URL = "/nav-db/main.db";
const DEFAULT_DB_FILENAME = "/nav-main.db";

export class BrowserNavDb {
  private constructor(
    readonly sqlite3: Sqlite3Static,
    readonly db: Database,
    readonly sourceUrl: string,
  ) {}

  static async open(sourceUrl = DEFAULT_NAV_DB_URL): Promise<BrowserNavDb> {
    const sqlite3 = await sqlite3InitModule();
    const response = await fetch(sourceUrl);
    if (!response.ok) {
      throw new Error(`failed to fetch nav db ${sourceUrl}: ${response.status} ${response.statusText}`);
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    sqlite3.capi.sqlite3_js_posix_create_file(DEFAULT_DB_FILENAME, bytes);
    const db = new sqlite3.oo1.DB(DEFAULT_DB_FILENAME, "r");
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
