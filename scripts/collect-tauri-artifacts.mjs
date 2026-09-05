import { cp, mkdir, readdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputDirectory = path.join(projectRoot, 'dist', 'installers');
const targets = [
  ['x64', 'x86_64-pc-windows-msvc'],
  ['x86', 'i686-pc-windows-msvc'],
  ['arm64', 'aarch64-pc-windows-msvc'],
];

async function collectFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await collectFiles(entryPath));
    } else {
      files.push(entryPath);
    }
  }

  return files;
}

await rm(outputDirectory, { recursive: true, force: true });
await mkdir(outputDirectory, { recursive: true });

for (const [architecture, target] of targets) {
  const bundleDirectory = path.join(projectRoot, 'src-tauri', 'target', target, 'release', 'bundle');
  const files = await collectFiles(bundleDirectory);

  for (const sourcePath of files) {
    const destinationName = `${architecture}-${path.basename(sourcePath)}`;
    await cp(sourcePath, path.join(outputDirectory, destinationName));
  }
}

console.log(`Артефакты собраны в ${path.relative(projectRoot, outputDirectory)}`);