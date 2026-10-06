// Local development settings from .convt-dev/services.env (written by
// scripts/db.sh up). Explicit environment variables win.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

export const repoRoot = (() => {
  let dir = import.meta.dir;
  while (!existsSync(join(dir, "scripts", "db.sh"))) {
    const up = dirname(dir);
    if (up === dir) throw new Error("cannot find the convt checkout");
    dir = up;
  }
  return dir;
})();

export const devDir = join(repoRoot, ".convt-dev");

export function servicesEnv(): Record<string, string> {
  const file = join(devDir, "services.env");
  const values: Record<string, string> = {};
  if (existsSync(file)) {
    for (const line of readFileSync(file, "utf8").split("\n")) {
      const m = line.match(/^([A-Z_]+)=(.*)$/);
      if (m) values[m[1]] = m[2];
    }
  }
  return {
    ...values,
    ...(Object.fromEntries(Object.entries(process.env).filter(([, v]) => v)) as Record<
      string,
      string
    >),
  };
}

export function requireSetting(name: string): string {
  const value = servicesEnv()[name];
  if (!value) throw new Error(`${name} is not set; run \`bun run db:up\` or set it`);
  return value;
}
