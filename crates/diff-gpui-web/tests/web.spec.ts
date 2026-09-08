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

  for (const theme of ["ayu-dark", "sage"]) {
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-set-theme", { detail: theme }),
    );
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(runtimeErrors).toEqual([]);

  frame.remove();
});
