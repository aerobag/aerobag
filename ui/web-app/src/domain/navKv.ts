const magic = new TextEncoder().encode("AEROBAGNAVKV0001");
const version = 1;
const headerLen = 48;
const entryLen = 8;

type Entry = {
  keyOffset: number;
  valueOffset: number;
};

export type NavKvPageProvider = (pageIndex: number) => Promise<Uint8Array>;

export class NavKvRoot {
  private readonly entries: Entry[];
  private readonly keyBytes: Uint8Array;
  readonly pageSize: number;
  readonly valueBytesLen: number;

  private constructor(entries: Entry[], keyBytes: Uint8Array, pageSize: number, valueBytesLen: number) {
    this.entries = entries;
    this.keyBytes = keyBytes;
    this.pageSize = pageSize;
    this.valueBytesLen = valueBytesLen;
  }

  static parse(rootBytes: Uint8Array): NavKvRoot {
    if (rootBytes.byteLength < headerLen) {
      throw new Error("nav_kv root is shorter than header");
    }
    for (let index = 0; index < magic.byteLength; index += 1) {
      if (rootBytes[index] !== magic[index]) {
        throw new Error("nav_kv root has invalid magic");
      }
    }
    const view = new DataView(rootBytes.buffer, rootBytes.byteOffset, rootBytes.byteLength);
    const actualVersion = view.getUint32(16, true);
    if (actualVersion !== version) {
      throw new Error(`unsupported nav_kv version ${actualVersion}`);
    }
    const realEntryCount = view.getUint32(20, true);
    const pageSize = view.getUint32(24, true);
    const entryTableOffset = view.getUint32(28, true);
    const keyBytesOffset = view.getUint32(32, true);
    const keyBytesLen = view.getUint32(36, true);
    const valueBytesLen = view.getUint32(40, true);
    if (pageSize === 0) {
      throw new Error("nav_kv page_size must be non-zero");
    }
    const entryCount = realEntryCount + 1;
    const entryBytesLen = entryCount * entryLen;
    if (entryTableOffset !== headerLen) {
      throw new Error("nav_kv entry table offset must follow header in v1");
    }
    if (keyBytesOffset !== entryTableOffset + entryBytesLen) {
      throw new Error("nav_kv key bytes offset does not follow entry table");
    }
    if (rootBytes.byteLength !== keyBytesOffset + keyBytesLen) {
      throw new Error("nav_kv root length does not match key bytes length");
    }
    const entries: Entry[] = [];
    for (let index = 0; index < entryCount; index += 1) {
      const offset = entryTableOffset + index * entryLen;
      entries.push({
        keyOffset: view.getUint32(offset, true),
        valueOffset: view.getUint32(offset + 4, true),
      });
    }
    const keyBytes = rootBytes.slice(keyBytesOffset);
    validateParts(entries, keyBytes, valueBytesLen);
    return new NavKvRoot(entries, keyBytes, pageSize, valueBytesLen);
  }

  get length() {
    return this.entries.length - 1;
  }

  getValueRange(key: string): { start: number; end: number } | null {
    const target = new TextEncoder().encode(key);
    let left = 0;
    let right = this.length;
    while (left < right) {
      const mid = left + Math.floor((right - left) / 2);
      const comparison = compareBytes(this.keyAt(mid), target);
      if (comparison < 0) {
        left = mid + 1;
      } else if (comparison > 0) {
        right = mid;
      } else {
        return this.valueRangeAt(mid);
      }
    }
    return null;
  }

  prefixKeys(prefix: string): string[] {
    const target = new TextEncoder().encode(prefix);
    let left = 0;
    let right = this.length;
    while (left < right) {
      const mid = left + Math.floor((right - left) / 2);
      if (compareBytes(this.keyAt(mid), target) < 0) {
        left = mid + 1;
      } else {
        right = mid;
      }
    }
    const out: string[] = [];
    for (let index = left; index < this.length; index += 1) {
      const key = this.keyAt(index);
      if (!startsWithBytes(key, target)) {
        break;
      }
      out.push(new TextDecoder().decode(key));
    }
    return out;
  }

  async extractValue(key: string, pageProvider: NavKvPageProvider): Promise<Uint8Array | null> {
    const range = this.getValueRange(key);
    if (!range || range.start === range.end) {
      return null;
    }
    const startPage = Math.floor(range.start / this.pageSize);
    const endPage = Math.floor((range.end - 1) / this.pageSize);
    const out = new Uint8Array(range.end - range.start);
    const pages = await Promise.all(
      Array.from({ length: endPage - startPage + 1 }, (_, offset) => pageProvider(startPage + offset)),
    );
    let writeOffset = 0;
    for (let pageIndex = startPage; pageIndex <= endPage; pageIndex += 1) {
      const page = pages[pageIndex - startPage];
      const pageStart = pageIndex * this.pageSize;
      const sliceStart = Math.max(0, range.start - pageStart);
      const sliceEnd = Math.min(this.pageSize, range.end - pageStart);
      if (sliceEnd > page.byteLength || sliceStart > sliceEnd) {
        throw new Error(`nav_kv value page ${pageIndex} is too short`);
      }
      out.set(page.slice(sliceStart, sliceEnd), writeOffset);
      writeOffset += sliceEnd - sliceStart;
    }
    return out;
  }

  private keyAt(index: number): Uint8Array {
    const start = this.entries[index].keyOffset;
    const end = this.entries[index + 1].keyOffset;
    return this.keyBytes.slice(start, end);
  }

  private valueRangeAt(index: number) {
    return {
      start: this.entries[index].valueOffset,
      end: this.entries[index + 1].valueOffset,
    };
  }
}

let sharedNavKvStorePromise: Promise<NavKvStore | null> | null = null;

export class NavKvStore {
  private readonly pageCache = new Map<number, Promise<Uint8Array>>();

  constructor(readonly root: NavKvRoot) {}

  static async open(): Promise<NavKvStore | null> {
    const rootResponse = await fetch("/nav-kv/root");
    if (!rootResponse.ok) {
      return null;
    }
    return new NavKvStore(NavKvRoot.parse(new Uint8Array(await rootResponse.arrayBuffer())));
  }

  async getBytes(key: string): Promise<Uint8Array | null> {
    return this.root.extractValue(key, (pageIndex) => this.getPage(pageIndex));
  }

  async getJson<T>(key: string): Promise<T | null> {
    const value = await this.getBytes(key);
    if (!value) {
      return null;
    }
    return JSON.parse(new TextDecoder().decode(value)) as T;
  }

  private getPage(pageIndex: number): Promise<Uint8Array> {
    const cached = this.pageCache.get(pageIndex);
    if (cached) {
      return cached;
    }
    const page = fetch(`/nav-kv/values/${pageIndex.toString().padStart(4, "0")}`).then(async (response) => {
      if (!response.ok) {
        throw new Error(`failed to fetch nav_kv page ${pageIndex}: ${response.status}`);
      }
      return new Uint8Array(await response.arrayBuffer());
    });
    this.pageCache.set(pageIndex, page);
    return page;
  }
}

export async function getNavKvStore(): Promise<NavKvStore | null> {
  if (!sharedNavKvStorePromise) {
    sharedNavKvStorePromise = NavKvStore.open();
  }
  return sharedNavKvStorePromise;
}

export async function loadNavKvJson<T>(key: string): Promise<T | null> {
  const store = await getNavKvStore();
  if (!store) {
    return null;
  }
  return store.getJson<T>(key);
}

function validateParts(entries: Entry[], keyBytes: Uint8Array, valueBytesLen: number) {
  if (entries.length < 2) {
    throw new Error("nav_kv needs at least one real entry plus sentinel");
  }
  const sentinel = entries[entries.length - 1];
  if (sentinel.keyOffset !== keyBytes.byteLength) {
    throw new Error("nav_kv sentinel key offset must equal key_bytes_len");
  }
  if (sentinel.valueOffset !== valueBytesLen) {
    throw new Error("nav_kv sentinel value offset must equal value_bytes_len");
  }
  for (let index = 0; index < entries.length - 1; index += 1) {
    const current = entries[index];
    const next = entries[index + 1];
    if (current.keyOffset >= next.keyOffset) {
      throw new Error("nav_kv key offsets must be strictly increasing");
    }
    if (current.valueOffset >= next.valueOffset) {
      throw new Error("nav_kv values must be non-empty and increasing");
    }
    if (next.keyOffset > keyBytes.byteLength) {
      throw new Error("nav_kv key offset exceeds key bytes length");
    }
    if (next.valueOffset > valueBytesLen) {
      throw new Error("nav_kv value offset exceeds value bytes length");
    }
    if (index > 0 && compareBytes(keySlice(entries, keyBytes, index - 1), keySlice(entries, keyBytes, index)) >= 0) {
      throw new Error("nav_kv keys must be strictly sorted");
    }
  }
}

function keySlice(entries: Entry[], keyBytes: Uint8Array, index: number) {
  return keyBytes.slice(entries[index].keyOffset, entries[index + 1].keyOffset);
}

function compareBytes(left: Uint8Array, right: Uint8Array) {
  const len = Math.min(left.byteLength, right.byteLength);
  for (let index = 0; index < len; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] - right[index];
    }
  }
  return left.byteLength - right.byteLength;
}

function startsWithBytes(value: Uint8Array, prefix: Uint8Array) {
  if (prefix.byteLength > value.byteLength) {
    return false;
  }
  for (let index = 0; index < prefix.byteLength; index += 1) {
    if (value[index] !== prefix[index]) {
      return false;
    }
  }
  return true;
}
