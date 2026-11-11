import { execFileSync, spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { TestProject } from "vitest/node";

export default async function setup(project: TestProject) {
  const root = fileURLToPath(new URL("../../../../", import.meta.url));
  const binary = execFileSync("cargo", ["build", "--message-format=json", "-p", "clankerdiff-cli", "--no-default-features", "--bin", "clankerdiff", "--target-dir", join(root, "target")], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] })
    .trim().split("\n").map((line) => JSON.parse(line))
    .find((message) => message.reason === "compiler-artifact" && message.target.name === "clankerdiff" && message.executable)?.executable;
  if (!binary) throw new Error("headless CLI executable not found");
  const repository = await mkdtemp(join(tmpdir(), "clankerdiff-browser-"));
  execFileSync("git", ["init", "-q", repository]);
  await writeFile(join(repository, "file.txt"), "old\n");
  execFileSync("git", ["-C", repository, "add", "."]);
  execFileSync("git", ["-C", repository, "-c", "user.name=Browser Test", "-c", "user.email=browser@example.test", "commit", "-qm", "initial"]);
  await writeFile(join(repository, "file.txt"), "new\n");
  const child = spawn(binary, ["serve", repository, "--listen", "127.0.0.1:0"], { stdio: ["ignore", "ignore", "pipe"] });
  const exited = new Promise<void>((resolve) => child.once("close", () => resolve()));
  const cleanup = async () => {
    child.kill("SIGINT");
    const timer = setTimeout(() => child.kill("SIGKILL"), 6000);
    try { await exited; } finally { clearTimeout(timer); await rm(repository, { recursive: true, force: true }); }
  };
  try {
    const url = await new Promise<string>((resolve, reject) => {
      let diagnostics = "";
      const timer = setTimeout(() => reject(new Error(`remote server startup timeout: ${diagnostics}`)), 30_000);
      child.once("error", (error) => { clearTimeout(timer); reject(error); });
      child.once("exit", () => { clearTimeout(timer); reject(new Error(`remote server exited: ${diagnostics}`)); });
      child.stderr.on("data", (chunk) => {
        diagnostics += chunk.toString();
        const match = diagnostics.match(/ws:\/\/127\.0\.0\.1:\d+\/ws/);
        if (match) { clearTimeout(timer); resolve(match[0]); }
      });
    });
    project.provide("remoteUrl", url);
    return cleanup;
  } catch (error) {
    await cleanup();
    throw error;
  }
}

declare module "vitest" {
  export interface ProvidedContext { remoteUrl: string; graphicsContext: "webgpu" | "webgl2" }
}
