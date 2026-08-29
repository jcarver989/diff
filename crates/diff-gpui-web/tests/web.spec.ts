import { expect, test } from "vitest";

const documentFixture = {
  repo_root: "/fixture",
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
  for (const theme of ["ayu-dark", "sage"]) {
    frame.contentDocument?.dispatchEvent(
      new CustomEvent("diff-review-set-theme", { detail: theme }),
    );
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(runtimeErrors).toEqual([]);

  frame.remove();
});
