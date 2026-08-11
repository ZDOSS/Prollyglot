import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");

const read = (path) => readFile(resolve(repositoryRoot, path), "utf8");
const [cargoToml, desktopPackageText, tauriConfigText, changelog, readme] = await Promise.all([
  read("Cargo.toml"),
  read("apps/desktop/package.json"),
  read("apps/desktop/src-tauri/tauri.conf.json"),
  read("CHANGELOG.md"),
  read("README.md")
]);

const workspaceSection = cargoToml.match(
  /\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/
)?.[1];
const cargoVersion = workspaceSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!cargoVersion) throw new Error("Cargo.toml is missing [workspace.package].version.");

const desktopPackage = JSON.parse(desktopPackageText);
const tauriConfig = JSON.parse(tauriConfigText);
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

if (!semver.test(cargoVersion)) {
  throw new Error(`Workspace version is not valid SemVer: ${cargoVersion}`);
}
if (desktopPackage.version !== cargoVersion) {
  throw new Error(
    `Version mismatch: Cargo.toml is ${cargoVersion}, apps/desktop/package.json is ${desktopPackage.version}.`
  );
}
if (Object.hasOwn(tauriConfig, "version")) {
  throw new Error("Remove tauri.conf.json.version so the desktop bundle inherits Cargo.toml.");
}
if (!changelog.includes(`## [${cargoVersion}]`)) {
  throw new Error(`CHANGELOG.md has no entry for ${cargoVersion}.`);
}
if (!readme.includes(`Current pre-release: **${cargoVersion}**`)) {
  throw new Error(`README.md does not identify ${cargoVersion} as the current pre-release.`);
}

console.log(`Prollyglot version ${cargoVersion} is synchronized.`);
