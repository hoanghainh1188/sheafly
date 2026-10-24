#!/usr/bin/env node
// Builds src-tauri/THIRD_PARTY_LICENSES.txt: the copyright notices and license texts of
// every npm package and Rust crate shipped in the app, plus the bundled font. The
// installers include the file, and Help → Third-Party Licenses opens it.
//
//   node scripts/third-party-licenses.mjs           write the file
//   node scripts/third-party-licenses.mjs --check   fail if it is out of date (CI)
//
// Needs `pnpm install` and `cargo fetch` first, so the packages' own license files are on
// disk. A package that ships no license file gets the standard text of its license.

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "src-tauri", "THIRD_PARTY_LICENSES.txt");
const standardTexts = join(root, "scripts", "license-texts");
const OWN_CRATES = new Set(["sheafly", "sheafly-engine", "sheafly-license"]);
// Standard texts (from SPDX license-list-data) in scripts/license-texts.
const STANDARD_PREFERENCE = ["MIT", "Apache-2.0", "BSD-3-Clause", "MPL-2.0"];
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

function run(command, args) {
  return execFileSync(command, args, { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
}

function clean(text) {
  return text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n")
    .trim();
}

/** License files a package ships, by file name, so the order never depends on the disk. */
function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && LICENSE_FILE.test(entry.name))
    .map((entry) => entry.name)
    .sort()
    .map((name) => ({ name, text: clean(readFileSync(join(dir, name), "utf8")) }));
}

/** The standard text for a license expression, for packages that ship none. */
function standardText(expression, authors) {
  const ids = new Set(expression.split(/\s+(?:OR|AND|WITH)\s+|[()/]/).map((id) => id.trim()).filter(Boolean));
  // Where the package offers a choice, Sheafly takes it under the first of these.
  const id = STANDARD_PREFERENCE.find((candidate) => ids.has(candidate));
  if (!id) throw new Error(`No license file and no standard text for "${expression}"`);
  const holder = authors.length ? authors.join(", ") : "the authors";
  const text = readFileSync(join(standardTexts, `${id}.txt`), "utf8")
    .replace("<year> ", "")
    .replace(/<copyright holders>|<owner>/, holder);
  return [{ name: `${id} (standard text; the package ships no license file)`, text: clean(text) }];
}

function rustCrates() {
  const meta = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked"]));
  const byId = new Map(meta.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(meta.resolve.nodes.map((node) => [node.id, node]));
  const start = meta.packages.find((pkg) => pkg.name === "sheafly").id;
  // Everything the app links: normal dependencies on any platform, not build or dev ones.
  const seen = new Set([start]);
  const queue = [start];
  while (queue.length) {
    for (const dep of nodes.get(queue.shift()).deps) {
      if (!dep.dep_kinds.some((kind) => kind.kind === null) || seen.has(dep.pkg)) continue;
      seen.add(dep.pkg);
      queue.push(dep.pkg);
    }
  }
  return [...seen]
    .map((id) => byId.get(id))
    .filter((pkg) => !OWN_CRATES.has(pkg.name))
    .map((pkg) => {
      const dir = dirname(pkg.manifest_path);
      const license = pkg.license ?? `see ${pkg.license_file}`;
      const files = licenseFiles(dir);
      return {
        kind: "Rust crate",
        name: pkg.name,
        version: pkg.version,
        license,
        source: pkg.repository ?? "",
        texts: files.length ? files : standardText(license, pkg.authors ?? []),
      };
    });
}

function npmPackages() {
  const listed = JSON.parse(run("pnpm", ["licenses", "list", "--prod", "--json"]));
  const packages = [];
  for (const [license, entries] of Object.entries(listed)) {
    for (const entry of entries) {
      entry.versions.forEach((version, index) => {
        const files = licenseFiles(entry.paths[index]);
        const author = typeof entry.author === "string" ? [entry.author] : [];
        packages.push({
          kind: "npm package",
          name: entry.name,
          version,
          license,
          source: entry.homepage ?? "",
          texts: files.length ? files : standardText(license, author),
        });
      });
    }
  }
  return packages;
}

function bundledFont() {
  return {
    kind: "font",
    name: "Be Vietnam Pro",
    version: "Regular",
    license: "OFL-1.1",
    source: "https://github.com/bettergui/BeVietnamPro",
    texts: [{ name: "OFL.txt", text: clean(readFileSync(join(root, "src", "fonts", "OFL.txt"), "utf8")) }],
  };
}

function render(packages) {
  const sorted = [...packages].sort(
    (a, b) => a.name.localeCompare(b.name, "en") || a.version.localeCompare(b.version, "en") || a.kind.localeCompare(b.kind),
  );
  // Packages with the very same license texts are listed together, so the file does not
  // repeat the Apache license a hundred times.
  const groups = new Map();
  for (const pkg of sorted) {
    const key = pkg.texts.map((file) => file.text).join("\n\f\n");
    if (!groups.has(key)) groups.set(key, { texts: pkg.texts, packages: [] });
    groups.get(key).packages.push(pkg);
  }
  const rule = "=".repeat(78);
  const parts = [
    "Sheafly: third-party licenses",
    "",
    "Sheafly is free software under the MIT license (see LICENSE below). It includes",
    "the following software and font, each under its own license, reproduced here as",
    "those licenses require. Generated by scripts/third-party-licenses.mjs; do not edit.",
    "",
    "Components under the Mozilla Public License 2.0 are used unmodified; their source",
    "code is available at the repository listed next to each one, and from crates.io.",
    "",
    `${sorted.length} components: ${sorted.filter((p) => p.kind === "npm package").length} npm packages, ` +
      `${sorted.filter((p) => p.kind === "Rust crate").length} Rust crates, 1 font.`,
    "",
    rule,
    "Sheafly LICENSE",
    rule,
    "",
    clean(readFileSync(join(root, "LICENSE"), "utf8")),
  ];
  for (const group of groups.values()) {
    parts.push("", rule);
    for (const pkg of group.packages) {
      const source = pkg.source ? `  ${pkg.source}` : "";
      parts.push(`${pkg.name} ${pkg.version} (${pkg.kind}, ${pkg.license})${source}`);
    }
    parts.push(rule);
    for (const file of group.texts) {
      parts.push("", `--- ${file.name} ---`, "", file.text);
    }
  }
  return `${parts.join("\n")}\n`;
}

// The app is MIT, so every shipped package must offer a license without strong copyleft
// (an "A OR GPL" package is fine: we take A). LGPL and MPL-2.0 (unmodified) are allowed.
const COPYLEFT = /\b(A?GPL|SSPL|EUPL|OSL|CC-BY-SA)\b/i;
function copyleftOnly(expression) {
  return expression
    .replace(/[()]/g, "")
    .split(/\s+OR\s+|\//i)
    .every((choice) => COPYLEFT.test(choice.replace(/LGPL/gi, "")));
}

const packages = [...npmPackages(), ...rustCrates(), bundledFont()];
const copyleft = packages.filter((pkg) => copyleftOnly(pkg.license));
if (copyleft.length) {
  console.error("These packages are only under a copyleft license the MIT app cannot ship:");
  for (const pkg of copyleft) console.error(`  ${pkg.name} ${pkg.version} (${pkg.license})`);
  process.exit(1);
}

const text = render(packages);
if (process.argv.includes("--check")) {
  const current = existsSync(output) ? readFileSync(output, "utf8") : "";
  if (current !== text) {
    console.error(
      "src-tauri/THIRD_PARTY_LICENSES.txt is out of date. Run `node scripts/third-party-licenses.mjs` and commit it.",
    );
    process.exit(1);
  }
} else {
  writeFileSync(output, text);
}
