import { expect, inject, test } from "vitest";

test("managed Rust WebSocket connection owns documents until disconnected", { timeout: 60_000 }, async () => {
  const frame = document.createElement("iframe");
  frame.src = "/index.html";
  frame.style.width = "1280px";
  frame.style.height = "800px";
  document.body.append(frame);
  try {
    type Bindings = {
      connect_remote(url: string): Promise<void>;
      disconnect_remote(): void;
      set_document_json(json: string): void;
    };
    const bindings = () => (frame.contentWindow as Window & { wasmBindings?: Bindings })?.wasmBindings;
    await expect.poll(() => bindings(), { timeout: 30_000 }).toBeDefined();
    await expect.poll(() => frame.contentDocument?.querySelectorAll("canvas").length).toBe(1);
    const states: string[] = [];
    frame.contentDocument!.addEventListener("diff-review-connection-state", (event) => {
      states.push(JSON.parse((event as CustomEvent).detail).state);
    });
    await bindings()!.connect_remote(inject("remoteUrl"));
    await expect.poll(() => states.includes("connected"), { timeout: 15_000 }).toBe(true);
    expect(() => bindings()!.set_document_json(JSON.stringify({ repo_root: "/conflict", files: [] }))).toThrow();
    bindings()!.disconnect_remote();
    await expect.poll(() => states.at(-1)).toBe("host");
    const applied: unknown[] = [];
    frame.contentDocument!.addEventListener("diff-review-document-applied", (event) => applied.push((event as CustomEvent).detail));
    bindings()!.set_document_json(JSON.stringify({ repo_root: "/host", files: [] }));
    await expect.poll(() => applied.length).toBe(1);
  } finally {
    frame.remove();
  }
});
