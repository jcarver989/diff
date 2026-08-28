import { expect, test } from "vitest";

const documentFixture = {
  repo_root: "/fixture",
  files: [],
};

test("starts the GPUI canvas in a real browser", async () => {
  const frame = document.createElement("iframe");
  frame.src = "/dist/index.html";
  frame.style.width = "1280px";
  frame.style.height = "800px";
  document.body.append(frame);

  await new Promise<void>((resolve, reject) => {
    frame.addEventListener("load", () => resolve(), { once: true });
    frame.addEventListener("error", () => reject(new Error("web fixture failed to load")), {
      once: true,
    });
  });

  await expect.poll(() => frame.contentDocument?.querySelectorAll("canvas").length).toBe(1);
  expect(JSON.stringify(documentFixture)).toContain("repo_root");

  frame.remove();
});
