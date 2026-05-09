import currentArtifactsJson from "@current-artifacts";

type CurrentArtifacts = {
  artifact_roots?: {
    packaged?: string;
    unpacked?: string;
  };
  bundles?: Array<{
    bundle_type?: string;
    filename?: string;
    checksum_sha256?: string;
  }>;
};

type BundlePackage = {
  id?: string;
  family_id?: string;
  filename?: string;
  relative_path?: string;
};

type BundleManifest = {
  packages?: BundlePackage[];
};

const PACKAGE_ROOT = "/packages";
const currentArtifacts = currentArtifactsJson as CurrentArtifacts;

let packageIndexPromise: Promise<Map<string, BundlePackage>> | null = null;

function requireArtifactRoots() {
  const packaged = currentArtifacts.artifact_roots?.packaged;
  const unpacked = currentArtifacts.artifact_roots?.unpacked;
  if (!packaged || !unpacked) {
    throw new Error("current_artifacts missing artifact_roots");
  }
  return { packaged, unpacked };
}

function joinUrl(...parts: string[]): string {
  return parts
    .map((part, index) => {
      if (index === 0) {
        return part.replace(/\/+$/g, "");
      }
      return part.replace(/^\/+|\/+$/g, "");
    })
    .filter((part) => part.length > 0)
    .join("/");
}

function zipStem(relativePath: string): string {
  if (!relativePath.endsWith(".zip")) {
    throw new Error(`expected zip package relative_path, got ${relativePath}`);
  }
  return relativePath.slice(0, -".zip".length);
}

function packageRelativePath(pkg: BundlePackage): string {
  const relativePath = pkg.relative_path ?? pkg.filename;
  if (!relativePath) {
    throw new Error(`package ${pkg.id ?? "(unknown)"} missing relative_path`);
  }
  return relativePath;
}

async function loadPackageIndex(): Promise<Map<string, BundlePackage>> {
  if (!packageIndexPromise) {
    packageIndexPromise = (async () => {
      const roots = requireArtifactRoots();
      const byId = new Map<string, BundlePackage>();
      for (const bundle of currentArtifacts.bundles ?? []) {
        if (!bundle.filename) {
          continue;
        }
        const response = await fetch(joinUrl(PACKAGE_ROOT, roots.packaged, bundle.filename), {
          cache: "no-cache",
        });
        if (!response.ok) {
          throw new Error(`failed to load bundle ${bundle.filename}: ${response.status}`);
        }
        const manifest = await response.json() as BundleManifest;
        for (const pkg of manifest.packages ?? []) {
          if (pkg.id) {
            byId.set(pkg.id, pkg);
          }
        }
      }
      return byId;
    })();
  }
  return packageIndexPromise;
}

export async function findPackageById(packageId: string): Promise<BundlePackage | null> {
  return (await loadPackageIndex()).get(packageId) ?? null;
}

export async function findPackageByFamily(familyId: string): Promise<BundlePackage | null> {
  for (const pkg of (await loadPackageIndex()).values()) {
    if (pkg.family_id === familyId) {
      return pkg;
    }
  }
  return null;
}

export async function packageRootUrl(packageId: string): Promise<string> {
  const pkg = await findPackageById(packageId);
  if (!pkg) {
    throw new Error(`package ${packageId} not found in active bundles`);
  }
  const roots = requireArtifactRoots();
  return joinUrl(PACKAGE_ROOT, roots.unpacked, zipStem(packageRelativePath(pkg)));
}

export async function packageResourceUrl(packageId: string, relativePath: string): Promise<string> {
  return joinUrl(await packageRootUrl(packageId), relativePath);
}

export async function familyPackageResourceUrl(familyId: string, relativePath: string): Promise<string> {
  const pkg = await findPackageByFamily(familyId);
  if (!pkg?.id) {
    throw new Error(`family ${familyId} not found in active bundles`);
  }
  return packageResourceUrl(pkg.id, relativePath);
}

export async function navKvResourceUrl(relativePath: string): Promise<string> {
  return familyPackageResourceUrl("nav-db", relativePath);
}

export async function resolveCoreResourceUrl(address: string): Promise<string> {
  if (address.startsWith("/nav-kv/")) {
    return navKvResourceUrl(address.slice("/nav-kv/".length));
  }
  if (address.startsWith("/fast-products/")) {
    const [packageId, ...rest] = address.slice("/fast-products/".length).split("/");
    return packageResourceUrl(packageId, rest.join("/"));
  }
  if (address.startsWith("/terrain-products/")) {
    const [packageId, ...rest] = address.slice("/terrain-products/".length).split("/");
    return packageResourceUrl(packageId, rest.join("/"));
  }
  if (address.startsWith("/sectional-packages/")) {
    const [packageId, ...rest] = address.slice("/sectional-packages/".length).split("/");
    return packageResourceUrl(packageId, rest.join("/"));
  }
  return address;
}
