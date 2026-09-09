import { expect, test } from "vitest";

const documentFixture = {
  repo_root: "/fixture",
  files: [],
};

const changedFixture = {
  repo_root: "/fixture-changed",
  files: [],
};

test("starts the GPUI canvas in a real browser", { timeout: 60_000 }, async () => {
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
  expect(JSON.stringify(documentFixture)).toContain("repo_root");

  const runtimeErrors: unknown[] = [];
  frame.contentWindow?.addEventListener("error", (event) => runtimeErrors.push(event.error));
  frame.contentWindow?.addEventListener("unhandledrejection", (event) =>
    runtimeErrors.push(event.reason),
  );

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
    const input = frame.contentDocument?.querySelector("textarea");
    expect(input).not.toBeNull();
    input?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    input?.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 100));
  };
  const complete = async (request_id: number) => {
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-repository-completed", {
        detail: JSON.stringify({ request_id, error: null }),
      }),
    );
    // DOM events enqueue work on GPUI's executor; let it consume the reply
    // before delivering the next keyboard action.
    await new Promise((resolve) => setTimeout(resolve, 100));
  };

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
  expect(runtimeErrors).toEqual([]);

  frame.remove();
});
