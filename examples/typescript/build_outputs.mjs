import { readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

function main() {
  const appRoot = dirname(fileURLToPath(import.meta.url));
  const repoRoot = resolve(appRoot, "..", "..");
  const inputsRoot = resolve(repoRoot, "examples", "inputs");
  const descriptorPath = resolve(
    repoRoot,
    "examples",
    "descriptors",
    "temporal_api.bin",
  );

  const exampleIds = process.argv.slice(2);
  const selectedIds =
    exampleIds.length > 0 ? exampleIds : discoverExampleIds(appRoot, inputsRoot);
  if (selectedIds.length === 0) {
    console.error("no TypeScript examples found");
    process.exit(1);
  }

  for (const exampleId of selectedIds) {
    const inputPath = inputPathForExample(inputsRoot, exampleId);
    const outputPath = resolve(appRoot, exampleId, "output.ts");
    const command = generatorCommand(repoRoot);

    const result = spawnSync(
      command[0],
      [
        ...command.slice(1),
        "generate",
        "--lang",
        "typescript",
        "--input",
        inputPath,
        "--descriptors",
        descriptorPath,
        "--output",
        outputPath,
        "--format",
      ],
      {
        cwd: repoRoot,
        stdio: "inherit",
      },
    );

    if (result.status !== 0) {
      process.exit(result.status ?? 1);
    }

    console.log(`Built ${outputPath} with nexus-api-gen`);
  }
}

function discoverExampleIds(appRoot, inputsRoot) {
  const languageDirectories = new Set(
    readdirSync(appRoot, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name),
  );

  const exampleIds = new Set(
    readdirSync(inputsRoot, { withFileTypes: true })
      .filter((entry) => entry.isFile() && entry.name.endsWith(".wit"))
      .map((entry) => entry.name.slice(0, -4))
      .filter((exampleId) => languageDirectories.has(exampleId)),
  );

  for (const entry of readdirSync(inputsRoot, { withFileTypes: true })) {
    if (
      entry.isDirectory() &&
      languageDirectories.has(entry.name) &&
      readdirSync(resolve(inputsRoot, entry.name)).includes("main.wit")
    ) {
      exampleIds.add(entry.name);
    }
  }

  return [...exampleIds].sort();
}

function inputPathForExample(inputsRoot, exampleId) {
  const flatPath = resolve(inputsRoot, `${exampleId}.wit`);
  if (readdirSync(inputsRoot).includes(`${exampleId}.wit`)) {
    return flatPath;
  }

  return resolve(inputsRoot, exampleId, "main.wit");
}

function generatorCommand(repoRoot) {
  if (process.env.NEXUS_API_GEN_BIN) {
    return [process.env.NEXUS_API_GEN_BIN];
  }

  return ["cargo", "run", "--quiet", "--"];
}

main();
