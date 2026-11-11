import { cdp, userEvent } from "vitest/browser";
import { expect, inject, test } from "vitest";

const documentFixture = {
  repo_root: "/fixture",
  files: [],
};

const changedFixture = {
  repo_root: "/fixture-changed",
  files: [],
};

test("starts the review canvas in a real browser", { timeout: 60_000 }, async () => {
  const frame = document.createElement("iframe");
  frame.src = "/index.html";
  frame.style.width = "1280px";
  frame.style.height = "800px";
  document.body.append(frame);

  await new Promise<void>((resolve, reject) => {
    frame.addEventListener("load", () => resolve(), { once: true });
    frame.addEventListener("error", () => reject(new Error("web fixture failed to load")), {
      once: true,
    });
  });

  await expect
    .poll(() => frame.contentDocument?.querySelectorAll("canvas").length, { timeout: 30_000 })
    .toBe(1);
  const doc = frame.contentDocument!;
  const runtimeErrors: unknown[] = [];
  frame.contentWindow?.addEventListener("error", (event) => runtimeErrors.push(event.error));
  frame.contentWindow?.addEventListener("unhandledrejection", (event) =>
    runtimeErrors.push(event.reason),
  );
  const canvas = doc.querySelector("canvas")!;
  await expect.poll(() => canvas.width).toBe(Math.round(1280 * frame.contentWindow!.devicePixelRatio));
  expect(canvas.getContext(inject("graphicsContext"))).not.toBeNull();
  await expect.poll(() => presentedColors(), { timeout: 15_000 }).toBeGreaterThan(50);
  frame.style.width = "960px";
  await expect.poll(() => canvas.width).toBe(Math.round(960 * frame.contentWindow!.devicePixelRatio));
  const winFrames = frame.contentWindow!;
  const requestFrame = winFrames.requestAnimationFrame;
  let requestedFrames = 0;
  winFrames.requestAnimationFrame = (callback) => {
    requestedFrames += 1;
    return requestFrame.call(winFrames, callback);
  };
  await new Promise((resolve) => setTimeout(resolve, 200));
  const settledFrames = requestedFrames;
  await new Promise((resolve) => setTimeout(resolve, 200));
  expect(requestedFrames).toBe(settledFrames);

  const acknowledgements: { revision: number | null; changed: boolean }[] = [];
  frame.contentDocument?.addEventListener("diff-review-document-applied", (event) => {
    acknowledgements.push(JSON.parse((event as CustomEvent).detail as string));
  });

  const push = (payload: unknown) =>
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-set-document", { detail: JSON.stringify(payload) }),
    );

  // The same document twice, then a revision envelope carrying new content.
  push(documentFixture);
  push(documentFixture);
  push({ revision: 2, document: changedFixture });

  await expect.poll(() => acknowledgements.length, { timeout: 10_000 }).toBe(3);
  expect(acknowledgements.map((ack) => ack.changed)).toEqual([true, false, true]);
  expect(acknowledgements.map((ack) => ack.revision)).toEqual([null, null, 2]);

  // Older and repeated revisions must leave the current content installed.
  push({ revision: 1, document: documentFixture });
  push({ revision: 2, document: documentFixture });
  push({ revision: 3, document: changedFixture });
  await expect.poll(() => acknowledgements.length, { timeout: 10_000 }).toBe(6);
  expect(acknowledgements.slice(3).map((ack) => ack.changed)).toEqual([false, false, false]);

  const requests: { request_id: number; action: unknown }[] = [];
  frame.contentDocument?.addEventListener("diff-review-repository-action", (event) => {
    requests.push(JSON.parse((event as CustomEvent).detail as string));
  });
  const key = async (key: string) => {
    const active = frame.contentDocument?.activeElement;
    expect(["CANVAS", "TEXTAREA"]).toContain(active?.tagName);
    await userEvent.keyboard(key);
    await new Promise((resolve) => setTimeout(resolve, 100));
  };
  const complete = async (request_id: number) => {
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-repository-completed", {
        detail: JSON.stringify({ request_id, error: null }),
      }),
    );
    // DOM events enqueue work in the viewer; let it consume the reply
    // before delivering the next keyboard action.
    await new Promise((resolve) => setTimeout(resolve, 100));
  };

  frame.contentDocument!.querySelector("canvas")!.focus();
  expect(frame.contentDocument!.activeElement?.tagName).toBe("CANVAS");
  await key("h");
  await key("a");
  await expect.poll(() => requests.length).toBe(1);
  push({ revision: 4, document: changedFixture });
  await expect.poll(() => acknowledgements.length).toBe(7);
  await key("a");
  expect(requests).toHaveLength(1); // An unrelated push cannot settle pending.

  push({ revision: 4, document: changedFixture, request_id: requests[0].request_id });
  await expect.poll(() => acknowledgements.length).toBe(8);
  await key("a");
  await expect.poll(() => requests.length).toBe(2);
  expect(requests[1].request_id).not.toBe(requests[0].request_id);
  await complete(requests[0].request_id);
  await key("a");
  expect(requests).toHaveLength(2); // A late reply cannot settle the next command.
  await complete(requests[1].request_id);
  await key("a");
  await expect.poll(() => requests.length).toBe(3);
  await complete(requests[2].request_id);

  const results: { request_id: number; handled: boolean; error: string | null }[] = [];
  frame.contentDocument?.addEventListener("diff-review-command-result", (event) => {
    results.push(JSON.parse((event as CustomEvent).detail as string));
  });
  const command = async (target: "diff" | "markdown", command: unknown) => {
    const request_id = results.length + 1;
    frame.contentDocument?.dispatchEvent(new CustomEvent("diff-review-command", {
      detail: JSON.stringify({ request_id, target, command }),
    }));
    await expect.poll(() => results.length).toBe(request_id);
    expect(results[request_id - 1].request_id).toBe(request_id);
    return results[request_id - 1];
  };
  const capabilities = (enabled: boolean) => {
    frame.contentDocument?.dispatchEvent(new CustomEvent("diff-review-set-capabilities", {
      detail: JSON.stringify({ repository: enabled, refresh: enabled, scope: enabled, submit: enabled, clipboard: enabled }),
    }));
  };

  expect((await command("diff", "stage_all")).handled).toBe(true);
  await expect.poll(() => requests.length).toBe(4);
  expect((await command("diff", "stage_all")).handled).toBe(false);
  await complete(requests[3].request_id);
  capabilities(false);
  expect((await command("diff", "stage_all")).handled).toBe(false);
  await key("a");
  expect(requests).toHaveLength(4);
  expect((await command("diff", "refresh")).handled).toBe(false);
  capabilities(true);

  const refreshes: { request_id: number }[] = [];
  frame.contentDocument?.addEventListener("diff-review-refresh", (event) => {
    refreshes.push(JSON.parse((event as CustomEvent).detail as string));
  });
  expect((await command("diff", "refresh")).handled).toBe(true);
  await expect.poll(() => refreshes.length).toBe(1);
  await complete(refreshes[0].request_id);
  expect((await command("diff", { review: "show_help" })).handled).toBe(true);
  expect((await command("diff", "stage_all")).handled).toBe(false);
  expect((await command("diff", { review: "cancel" })).handled).toBe(true);
  expect((await command("markdown", "approve")).error).toContain("not active");

  capabilities(false);
  frame.contentDocument?.dispatchEvent(new CustomEvent("markdown-review-set-document", {
    detail: JSON.stringify({ source: "# Heading\n\n```just\ngreet name:\n\techo {{ name }}\n```" }),
  }));
  expect((await command("markdown", "approve")).handled).toBe(false);
  expect((await command("diff", "stage_all")).error).toContain("not active");
  expect((await command("markdown", { review: "begin_comment" })).handled).toBe(true);
  expect((await command("markdown", "next_heading")).handled).toBe(false);
  expect((await command("markdown", { review: "cancel" })).handled).toBe(true);
  expect((await command("markdown", "request_changes")).handled).toBe(false);
  capabilities(true);
  const markdownSubmissions: unknown[] = [];
  frame.contentDocument?.addEventListener("markdown-review-submit", (event) => {
    markdownSubmissions.push(JSON.parse((event as CustomEvent).detail as string));
  });
  expect(await command("markdown", "request_changes")).toMatchObject({ handled: true, error: null });
  expect(await command("markdown", "approve")).toMatchObject({ handled: true, error: null });
  await expect.poll(() => markdownSubmissions.length).toBe(2);
  expect(markdownSubmissions).toMatchObject([
    { decision: "ChangesRequested" },
    { decision: "Approved" },
  ]);
  expect((await command("markdown", { review: "begin_comment" })).handled).toBe(true);
  await expect.poll(() => doc.activeElement?.tagName).toBe("TEXTAREA");
  await cdp().send("Input.imeSetComposition", { text: "あ", selectionStart: 1, selectionEnd: 1 });
  expect((await command("markdown", "approve")).handled).toBe(false);
  await cdp().send("Input.insertText", { text: "あ" });
  await key("{Enter}");
  expect((await command("markdown", "request_changes")).handled).toBe(true);
  await expect.poll(() => markdownSubmissions.length).toBe(3);
  expect(markdownSubmissions[2]).toMatchObject({ comments: [{ body: "あ" }] });
  push({ revision: 5, document: changedFixture });
  expect((await command("diff", "refresh")).handled).toBe(true);
  await expect.poll(() => refreshes.length).toBe(2);
  await complete(refreshes[1].request_id);

  for (const theme of ["ayu-dark", "sage"]) {
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-set-theme", { detail: theme }),
    );
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
  const sourceFile = {
    path: "source.rs", old_path: "source.rs", status: "Modified", staged: "Staged",
    binary: false, mode: null, no_newline_at_end: false,
    old_source: { Ok: "// out-of-hunk before\nlet old_only = 1;\n// out-of-hunk after\n" },
    new_source: { Ok: "// out-of-hunk before\nlet new_only = 2;\n// out-of-hunk after\n" },
    hunks: [{
      header: "@@ -2 +2 @@", function_context: null,
      old_start: 2, old_count: 1, new_start: 2, new_count: 1,
      lines: [
        { kind: "Removed", text: "let old_only = 1;", old_line_no: 2, new_line_no: null, no_newline: false },
        { kind: "Added", text: "let new_only = 2;", old_line_no: null, new_line_no: 2, no_newline: false },
      ],
    }],
  };
  const applied = acknowledgements.length;
  push({ repo_root: "/source-fixture", files: [sourceFile] });
  await expect.poll(() => acknowledgements.length).toBe(applied + 1);
  await command("diff", { focus: "diff" });
  const repositoryRequests = requests.length;
  await key("o");
  expect((await command("diff", "toggle_full_file")).handled).toBe(false);
  expect((await command("diff", { review: "begin_comment" })).handled).toBe(false);
  await key("o");
  expect((await command("diff", { review: "begin_comment" })).handled).toBe(true);
  await key("o");
  expect((await command("diff", "toggle_source_view")).handled).toBe(false);
  await command("diff", { review: "cancel" });
  expect((await command("diff", "toggle_source_view")).handled).toBe(true);
  await command("diff", { review: "show_help" });
  await key("o");
  expect((await command("diff", "toggle_source_view")).handled).toBe(false);
  await command("diff", { review: "cancel" });
  expect((await command("diff", "toggle_full_file")).handled).toBe(false);
  expect((await command("diff", "toggle_source_view")).handled).toBe(true);
  const { old_source, new_source, ...patchOnly } = sourceFile;
  push({ repo_root: "/patch-fixture", files: [patchOnly] });
  await expect.poll(() => acknowledgements.length).toBe(applied + 2);
  expect((await command("diff", "toggle_source_view")).handled).toBe(true);
  expect((await command("diff", "toggle_source_view")).handled).toBe(true);
  expect(requests).toHaveLength(repositoryRequests);
  type Bindings = {
    start(): void;
    stop(): void;
    set_document_json(json: string): void;
    set_theme(name: string): void;
  };
  const win = frame.contentWindow as Window & { wasmBindings: Bindings };
  const bindings = win.wasmBindings;
  expect(() => bindings.set_document_json("{")).toThrow();
  bindings.stop();
  expect(doc.querySelector("canvas, textarea")).toBeNull();
  expect(() => bindings.stop()).toThrow();
  const storage = Object.getOwnPropertyDescriptor(win, "localStorage")!;
  Object.defineProperty(win, "localStorage", {
    configurable: true,
    get() { throw new DOMException("test storage denied", "SecurityError"); },
  });
  try {
    bindings.start();
    expect(() => bindings.start()).toThrow();
    const before = acknowledgements.length;
    bindings.set_document_json(JSON.stringify(documentFixture));
    bindings.set_theme("sage");
    await expect.poll(() => acknowledgements.length).toBe(before + 1);
    await expect.poll(() => doc.querySelector("canvas")?.width).toBe(Math.round(960 * win.devicePixelRatio));
    expect(doc.querySelectorAll("textarea")).toHaveLength(1);
    bindings.stop();
    expect(doc.querySelector("canvas, textarea")).toBeNull();
  } finally {
    Object.defineProperty(win, "localStorage", storage);
  }
  expect(runtimeErrors).toEqual([]);

  frame.remove();
});

async function presentedColors(): Promise<number> {
  const { data: base64 } = await cdp().send("Page.captureScreenshot", { format: "png" });
  const bitmap = await createImageBitmap(new Blob([Uint8Array.fromBase64(base64)], { type: "image/png" }));
  try {
    const readback = document.createElement("canvas");
    readback.width = bitmap.width;
    readback.height = bitmap.height;
    const context = readback.getContext("2d")!;
    context.drawImage(bitmap, 0, 0);
    const { data } = context.getImageData(0, 0, readback.width, readback.height);
    const colors = new Set<number>();
    for (let i = 0; i < data.length; i += 16) {
      colors.add((data[i] << 16) | (data[i + 1] << 8) | data[i + 2]);
    }
    return colors.size;
  } finally {
    bitmap.close();
  }
}
