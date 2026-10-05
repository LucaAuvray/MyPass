// npm run release — builds the signed .msi and writes the updater manifest
// (latest.json) next to it. The version comes from src-tauri/Cargo.toml only.
// Deploying both files to the container is described in CLAUDE.md.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const fail = (msg) => {
  console.error(`release: ${msg}`);
  process.exit(1);
};

const version = readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version = "(.+)"/m)?.[1];
if (!version) fail("no version in src-tauri/Cargo.toml");

const key = join(homedir(), ".tauri", "mypass.key");
if (!existsSync(key)) fail(`signing key not found: ${key} (restore it from the vault copy)`);

const build = spawnSync("npm run tauri build", {
  shell: true,
  stdio: "inherit",
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "" },
});
if (build.status !== 0) fail("tauri build failed");

const dir = "src-tauri/target/release/bundle/msi";
const msi = `MyPass_${version}_x64_en-US.msi`;
if (!existsSync(join(dir, `${msi}.sig`))) fail(`${msi}.sig missing: was the build signed?`);

const manifest = {
  version,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(join(dir, `${msi}.sig`), "utf8"),
      url: `https://mypass-luca.tail7687c9.ts.net/download/${msi}`,
    },
  },
};
writeFileSync(join(dir, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`release: ${version} → ${dir}/${msi} + latest.json`);
