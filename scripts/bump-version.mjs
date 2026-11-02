// Increments the app patch version in src-tauri/tauri.conf.json so every
// built exe always offers users an upgrade (1.0.0 -> 1.0.1 -> ...).
// Keeps src-tauri/Cargo.toml aligned so all version labels agree.
import { readFileSync, writeFileSync } from 'node:fs';

const path = 'src-tauri/tauri.conf.json';
const conf = JSON.parse(readFileSync(path, 'utf8'));
const [maj, min, patch] = String(conf.version).split('.').map(Number);
conf.version = `${maj}.${min}.${(patch || 0) + 1}`;
writeFileSync(path, JSON.stringify(conf, null, 2) + '\n');

const cargoPath = 'src-tauri/Cargo.toml';
const cargo = readFileSync(cargoPath, 'utf8');
writeFileSync(cargoPath, cargo.replace(/version = "[^"]+"/, `version = "${conf.version}"`));

console.log('version ->', conf.version);
