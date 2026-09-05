import { cp, mkdir, readdir, rm, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const outputDirectory = path.join(projectRoot, "dist", "installers");

const targets = [
  ["x64", "x86_64-pc-windows-msvc"],
  ["x86", "i686-pc-windows-msvc"],
  ["arm64", "aarch64-pc-windows-msvc"],
];

const platformMapping = {
  x64: "windows-x86_64",
  x86: "windows-i686",
  arm64: "windows-aarch64",
};

async function collectFiles(directory) {
  try {
    const entries = await readdir(directory, { withFileTypes: true });
    const files = [];

    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        files.push(...(await collectFiles(entryPath)));
      } else {
        files.push(entryPath);
      }
    }

    return files;
  } catch (error) {
    if (error.code === "ENOENT") {
      console.log(`[Инфо] Путь не найден: ${directory}`);
      return [];
    }
    throw error;
  }
}

async function getTauriVersion() {
  try {
    const configPath = path.join(projectRoot, "src-tauri", "tauri.conf.json");
    const configRaw = await readFile(configPath, "utf-8");
    const config = JSON.parse(configRaw);
    return config.version || "1.0.0";
  } catch (e) {
    console.log(
      "[Предупреждение] Не удалось прочитать версию из tauri.conf.json, используется 1.0.0",
    );
    return "1.0.0";
  }
}

await rm(outputDirectory, { recursive: true, force: true });
await mkdir(outputDirectory, { recursive: true });

const version = await getTauriVersion();

const DEPLOY_BASE_URL =
  `https://github.com/CRUSTALiYm/ASCII/releases/download/v${version}`;

const latestJson = {
  version: version,
  notes: `Релиз версии ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {},
};

const signatures = {};
const updateArtifacts = [];

for (const [architecture, target] of targets) {
  const bundleDirectory = path.join(
    projectRoot,
    "src-tauri",
    "target",
    target,
    "release",
    "bundle",
  );
  const files = await collectFiles(bundleDirectory);

  for (const sourcePath of files) {
    const baseName = path.basename(sourcePath);
    const destinationName = `${architecture}-${baseName}`;
    await cp(sourcePath, path.join(outputDirectory, destinationName));

    if (baseName.endsWith(".exe.sig")) {
      const sigContent = await readFile(sourcePath, "utf-8");
      signatures[architecture] = sigContent.trim();
    } else if (baseName.endsWith(".exe") && !baseName.includes("msi")) {
      updateArtifacts.push({
        architecture,
        fileName: destinationName,
      });
    }
  }
}

for (const artifact of updateArtifacts) {
  const tauriPlatform = platformMapping[artifact.architecture];
  const signature = signatures[artifact.architecture];

  if (tauriPlatform && signature) {
    latestJson.platforms[tauriPlatform] = {
      url: `${DEPLOY_BASE_URL}/${artifact.fileName}`,
      signature: signature,
    };
  }
}

if (Object.keys(latestJson.platforms).length > 0) {
  const latestJsonPath = path.join(outputDirectory, "latest.json");
  await writeFile(latestJsonPath, JSON.stringify(latestJson, null, 2), "utf-8");
  console.log(
    `[Успех] Файл latest.json успешно сгенерирован в ${path.relative(projectRoot, latestJsonPath)}`,
  );
} else {
  console.log(
    "[Предупреждение] Не найдено подходящих .sig или .exe файлов. Убедитесь, что настроена подпись билдов.",
  );
}

console.log(
  `Артефакты собраны в ${path.relative(projectRoot, outputDirectory)}`,
);
