import type { Plugin } from "vite";
import { resolve } from "path";
import { readdirSync, readFileSync } from "fs";
import * as semver from "semver";

function getNewestWheel(distPath: string): string | null {
  const files = readdirSync(distPath).filter((f: string) => f.endsWith(".whl"));
  if (files.length === 0) return null;

  files.sort((a: string, b: string) => {
    const verA = a.match(/-(\d+\.\d+\.\d+)/)?.[1] ?? "0.0.0";
    const verB = b.match(/-(\d+\.\d+\.\d+)/)?.[1] ?? "0.0.0";
    return semver.rcompare(verA, verB);
  });

  return files[0];
}

export function externalDistPlugin(): Plugin {
  return {
    name: "external-dist",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        const url = req.url ?? "";
        const pathname = url.split("?")[0];

        if (pathname.startsWith("/dist/")) {
          // Go up three levels: web/src/plugins -> web/src -> web -> project root
          const distPath = resolve(__dirname, "..", "..", "..", "dist");

          try {
            let filename: string;
            const requested = pathname.slice(6);

            const isDummyRequest = /^hypercutter-\d+\.\d+\.\d+-py3-none-any\.whl$/.test(
              requested,
            );

            if (!requested || isDummyRequest) {
              const newest = getNewestWheel(distPath);
              if (!newest) {
                next();
                return;
              }
              filename = newest;
            } else if (requested.endsWith(".whl") && requested.startsWith("hypercutter-")) {
              const versionMatch = requested.match(/^hypercutter-([^-]+)-py3-none-any\.whl$/);
              const version = versionMatch?.[1];

              if (version) {
                const files = readdirSync(distPath).filter(
                  (f: string) => f.startsWith(`hypercutter-${version}-`) && f.endsWith(".whl"),
                );
                if (files.length > 0) {
                  filename = files[0];
                } else {
                  next();
                  return;
                }
              } else {
                next();
                return;
              }
            } else {
              next();
              return;
            }

            const filePath = resolve(distPath, filename);
            const content = readFileSync(filePath);
            _res.setHeader("Content-Type", "application/zip");
            _res.setHeader("Content-Disposition", `attachment; filename="${filename}"`);
            _res.end(content);
            return;
          } catch (e) {
            console.error("[dist-plugin] Error:", e);
            next();
          }
        }
        next();
      });
    },
  };
}